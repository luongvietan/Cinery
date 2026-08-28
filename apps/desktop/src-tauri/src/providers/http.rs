use std::io::Read;
use std::time::Duration;

pub trait HttpTransport: Send + Sync {
    fn post_json(&self, endpoint: &str, bearer_token: &str, body: &serde_json::Value) -> Result<serde_json::Value, String>;
    fn get_json(&self, endpoint: &str, bearer_token: &str) -> Result<serde_json::Value, String>;
    fn get_bytes(&self, endpoint: &str, bearer_token: &str, max_bytes: usize) -> Result<Vec<u8>, String>;
}

pub struct UreqTransport {
    agent: ureq::Agent,
}

impl UreqTransport {
    pub fn new(timeout: Duration) -> Self {
        Self {
            agent: ureq::AgentBuilder::new().timeout(timeout).build(),
        }
    }

    fn request_error(error: ureq::Error) -> String {
        match error {
            ureq::Error::Status(code, response) => {
                let body = response.into_string().unwrap_or_default();
                format!("HTTP {code}: {body}")
            }
            ureq::Error::Transport(error) => error.to_string(),
        }
    }
}

impl HttpTransport for UreqTransport {
    fn post_json(
        &self,
        endpoint: &str,
        bearer_token: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        self.agent
            .post(endpoint)
            .set("Authorization", &format!("Bearer {bearer_token}"))
            .set("Content-Type", "application/json")
            .send_json(body)
            .map_err(Self::request_error)?
            .into_json()
            .map_err(|error| error.to_string())
    }

    fn get_json(&self, endpoint: &str, bearer_token: &str) -> Result<serde_json::Value, String> {
        self.agent
            .get(endpoint)
            .set("Authorization", &format!("Bearer {bearer_token}"))
            .call()
            .map_err(Self::request_error)?
            .into_json()
            .map_err(|error| error.to_string())
    }

    fn get_bytes(
        &self,
        endpoint: &str,
        bearer_token: &str,
        max_bytes: usize,
    ) -> Result<Vec<u8>, String> {
        let response = self
            .agent
            .get(endpoint)
            .set("Authorization", &format!("Bearer {bearer_token}"))
            .call()
            .map_err(Self::request_error)?;
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take((max_bytes + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() > max_bytes {
            return Err(format!("response exceeds {max_bytes} byte limit"));
        }
        Ok(bytes)
    }
}
