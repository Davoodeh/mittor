use paho_mqtt as mqtt;

const KEEP_ALIVE: u64 = 10;
const DEFAULT_MQTT_PORT: u16 = 1883;

/// A wrapper for `http_types::Url` with to add integration, String-based validation and helpers.
#[derive(Debug, Clone)]
pub struct Url(http_types::Url);

impl Url {
    pub fn value(&self) -> &http_types::Url {
        &self.0
    }

    // Return plain `scheme://host:port/` without anything else.
    pub fn stripped(&self) -> String {
        let u = &self.0;
        format!(
            "{}://{}:{}/",
            u.scheme(),
            u.host_str().unwrap(),
            u.port().unwrap_or(DEFAULT_MQTT_PORT),
        )
    }

    pub fn connect_options(&self) -> mqtt::ConnectOptions {
        let mut c = mqtt::ConnectOptionsBuilder::new();
        c.keep_alive_interval(std::time::Duration::from_secs(KEEP_ALIVE))
            .mqtt_version(mqtt::MQTT_VERSION_3_1_1)
            .clean_session(false);
        let username = self.value().username();
        if !username.is_empty() {
            c.user_name(username);
        }
        if let Some(v) = self.value().password() {
            c.password(v);
        }
        c.finalize()
    }
}

impl std::str::FromStr for Url {
    type Err = http_types::url::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // try to add tcp to the url before trying to parse it
        let mut s = s.to_owned();
        if !s.starts_with("tcp://") {
            s.insert_str(0, "tcp://");
        }
        let url = http_types::Url::from_str(&s);
        Ok(Self(match url {
            Ok(v) if v.port() == None => v,
            Ok(mut v) => {
                v.set_port(Some(DEFAULT_MQTT_PORT)).unwrap();
                v
            }
            Err(e) => return Err(e),
        }))
    }
}

/// AsyncClient wrapper which may or may not exist (methods work or just ignore the calls).
pub struct OptionalClient(pub Option<mqtt::AsyncClient>);

impl OptionalClient {
    pub fn new(url: Option<String>) -> Self {
        Self(match url {
            Some(v) => Some(mqtt::AsyncClient::new(v).unwrap()),
            None => None,
        })
    }

    pub async fn connect(&self) -> Result<(), mqtt::Error> {
        if let Some(v) = &self.0 {
            // TODO add LWT for redirect.
            println!("Connecting to the redirect server...",);
            v.connect(None).await?;
        }
        Ok(())
    }

    pub async fn publish(&self, message: mqtt::Message) -> Result<(), mqtt::Error> {
        if let Some(v) = &self.0 {
            v.publish(message).await?;
        }
        Ok(())
    }

    pub async fn disconnect(&self) -> Result<(), mqtt::Error> {
        if let Some(v) = &self.0 {
            v.disconnect(None).await?;
        }
        Ok(())
    }
}

impl From<Option<Url>> for OptionalClient {
    fn from(o: Option<Url>) -> Self {
        Self::new(match o {
            Some(v) => Some(v.stripped().to_string()),
            None => None,
        })
    }
}
