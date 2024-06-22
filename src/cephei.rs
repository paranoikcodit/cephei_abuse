use reqwest::{
    header::{self, HeaderMap},
    Client, Proxy,
};
use serde_json::{json, Value};

use crate::Session;

#[derive(Clone)]
pub struct Cephei {
    inner: Client,
}

impl Cephei {
    pub async fn auth(session: Session, proxy: String) -> anyhow::Result<Cephei> {
        let init_data = session
            .get_init_data("Cephei_fi_bot".to_string(), "Farm".to_string())
            .await?;

        let access_token = reqwest::Client::default()
            .post("https://api-miniapp.cephei.fi/auth")
            .json(&json!({"initData": init_data}))
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?["content"]
            .as_str()
            .unwrap()
            .to_string();

        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, access_token.parse().unwrap());

        Ok(Self {
            inner: Client::builder()
                .proxy(Proxy::all(proxy)?)
                .default_headers(headers)
                .build()?,
        })
    }

    pub async fn register(
        &self,
        nickname: String,
        invite_code: Option<String>,
    ) -> anyhow::Result<Value> {
        self.inner
            .post("https://api-miniapp.cephei.fi/account/register")
            .json(&json!({"nickname": nickname, "invite_code": invite_code}))
            .send()
            .await?
            .json()
            .await
            .map_err(|e| e.into())
    }

    pub async fn start_farming(&self) -> anyhow::Result<()> {
        self.inner
            .get("https://api-miniapp.cephei.fi/farming/start")
            .send()
            .await?;

        Ok(())
    }

    pub async fn claim_farming(&self) -> anyhow::Result<()> {
        self.inner
            .get("https://api-miniapp.cephei.fi/farming/claim")
            .send()
            .await?;

        Ok(())
    }

    pub async fn get_tasks(&self) -> anyhow::Result<Value> {
        return self
            .inner
            .get("https://api-miniapp.cephei.fi/tasks/list")
            .send()
            .await?
            .json()
            .await
            .map_err(|e| e.into());
    }

    pub async fn check_task(&self, id: &str) -> anyhow::Result<()> {
        self.inner
            .get(format!("https://api-miniapp.cephei.fi/tasks/{id}/verify"))
            .send()
            .await?;

        Ok(())
    }
    pub async fn claim_task(&self, id: &str) -> anyhow::Result<()> {
        self.inner
            .get(format!("https://api-miniapp.cephei.fi/tasks/{id}/claim"))
            .send()
            .await?;

        Ok(())
    }
}
