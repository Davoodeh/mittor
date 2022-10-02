use clap::{arg, command, Parser};
use futures_util::StreamExt;
use paho_mqtt as mqtt;

use mittor::{OptionalClient, Url};

const MESSAGE_BUFFER_SIZE: usize = 5;
const RECONNECT_DELAY: u64 = 1000;
const DEFAULT_QOS: i32 = 0;
const DEFAULT_CLIENT_ID: &str = "MetalLeech";

#[derive(Parser, Debug)]
#[command(
    about,
    long_about = "Echo or mirror select topics of an MQTT server to the given redirect hosts"
)]
struct Args {
    #[arg(
        value_parser = clap::value_parser!(Url),
        help = "the upstream server to mirror from\n\
                (e.g. `localhost` or `tcp://localhost:1883`, `user:pass@localhost`)"
    )]
    source_host: Url,
    #[arg(
        short, long,
        value_parser = clap::value_parser!(Url),
        help = "addresses of hosts to redirect to\n\
                (e.g. `localhost` or `tcp://localhost:1883`, `user:pass@localhost`)"
    )]
    redirect_host: Option<Url>,
    #[arg(short, long, required = true, help = "the topics to listen to")]
    topics: Vec<String>,
    #[arg(
        short, long,
        value_parser = clap::value_parser!(i32).range(0..3),
        help = "the QoS for each topic, ordered\n\
                (the count must be the same as topics' otherwise uses default)\n\
                \n\
                [default: 0 for all the topics]"
    )]
    qos: Vec<i32>,
    #[arg(
        short, long,
        default_value = DEFAULT_CLIENT_ID,
        help = "client ID of mirrors sending requests to the redirect hosts"
    )]
    client: String,
}

impl Args {
    pub fn parse() -> Self {
        let mut candidate = <Self as Parser>::parse();
        if candidate.topics.len() != candidate.qos.len() {
            println!(
                "The number of topics doesn't match the number of QOS' given. \
                 Replacing with all {}s...",
                DEFAULT_QOS,
            );
            candidate.qos = (0..candidate.topics.len()).map(|_| DEFAULT_QOS).collect();
        }
        candidate
    }
}

impl std::fmt::Display for Args {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        macro_rules! tab {
            () => {
                write!(f, "    ")?;
            };
            (close) => {
                tab!(raw "}},\n");
            };
            ($e1:expr) => {
                tab!(raw "{:?}: {{,\n", $e1);
            };
            ($e1:expr, $e2:expr) => {
                tab!(raw "{:?}: {:?},\n", $e1, $e2);
            };
            (raw $($tt:tt)+) => {
                tab!();
                write!(f, $($tt)+)?;
            };
        }
        let redirect_host = if let Some(v) = &self.redirect_host {
            Some(v.stripped())
        } else {
            None
        };

        write!(f, "Inputs {{\n")?;
        tab!("client", self.client);
        tab!("source_host", self.source_host.stripped());
        tab!("redirect_host", redirect_host);
        tab!("topics_qos");
        for i in 0..self.topics.len() {
            tab!();
            tab!(self.topics[i], self.qos.get(i));
        }
        tab!(close);
        write!(f, "}}\n")?;
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    println!("{}", args);

    println!("Attaching to {}...", args.source_host.value());
    // create the leech client for the source host to listen to.
    let create_options = mqtt::CreateOptionsBuilder::new()
        .server_uri(args.source_host.stripped())
        .client_id(args.client)
        .finalize();
    let mut source_client = mqtt::AsyncClient::new(create_options).unwrap();

    // create the client for pubing to the redirect server.
    let redirect_oclient = OptionalClient::from(args.redirect_host);

    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let mut source_stream = source_client.get_stream(MESSAGE_BUFFER_SIZE);
        // create the connection and give user/pass depending on the URL.
        source_client
            .connect(args.source_host.connect_options())
            .await?;
        source_client
            .subscribe_many(args.topics.as_slice(), args.qos.as_slice())
            .await?;

        redirect_oclient.connect().await?;

        println!("Waiting for messages in any of topics: {:?}", args.topics);
        // TODO handle ^C to "gracefully" disconnect (client.disconnect(?).await?)
        loop {
            match source_stream.next().await {
                Some(Some(message)) => {
                    let topic = message.topic();
                    let payload = message.payload_str().to_string();
                    let qos = message.qos();

                    println!("{} (QOS: {}): {}", topic, qos, payload);

                    let _ = redirect_oclient.publish(message); // no need to wait on it
                }
                Some(None) => {
                    println!("Connection lost!");
                    while let Err(err) = source_client.reconnect().await {
                        println!("Reconnecting error: {}", err);
                        tokio::time::sleep(std::time::Duration::from_millis(RECONNECT_DELAY)).await;
                    }
                }
                None => break,
            }
        }

        redirect_oclient.disconnect().await?;

        // Explicit return type for the async block
        Ok::<(), mqtt::Error>(())
    })?;

    Ok(())
}
