use std::{
    env,
    fs::File,
    io::{self, BufRead, Read, Write},
    path::{Path, PathBuf},
};

use anyhow::anyhow;
use grammers_client::{Client, Config, InitParams, SignInError};
use pyo3::{
    types::{PyAnyMethods, PyModule},
    Bound, Python,
};

pub const PYTHON_PROGRAM: &str = include_str!("../converter.py");

/**
*     let data = pyo3::prelude::Python::with_gil(|py| {
       let converter =
           PyModule::from_code_bound(py, PYTHON_PROGRAM, "converter.py", "converter.py")?;

       let data: Vec<u8> = converter
           .getattr("convert_to_session")?
           .call1((tdata,))?
           .extract()?;

       Result::<Vec<u8>, pyo3::PyErr>::Ok(data)
   })?;

   grammers_session::Session::load(&data).map_err(|e| e.into())

*
*/

#[derive(Clone, Debug)]
pub enum SessionSource {
    TDestkop,
    Pyrogram,
    Telethon,
}

pub fn detect_session_format(
    converter: Bound<PyModule>,
    path: &str,
) -> anyhow::Result<SessionSource> {
    let data: String = converter
        .getattr("detect_session_format")?
        .call1((path,))?
        .extract()?;

    match data.as_str() {
        "Telethon" => Ok(SessionSource::Telethon),
        "Pyrogram" => Ok(SessionSource::Pyrogram),
        "TData" => Ok(SessionSource::TDestkop),
        _ => Err(anyhow!("Unknown session format")),
    }
}

fn prompt(message: &str) -> anyhow::Result<String> {
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    stdout.write_all(message.as_bytes())?;
    stdout.flush()?;

    let stdin = io::stdin();
    let mut stdin = stdin.lock();

    let mut line = String::new();
    stdin.read_line(&mut line)?;
    Ok(line)
}

pub fn convert_tdata_to_session(
    converter: Bound<PyModule>,
    tdata: &str,
) -> anyhow::Result<Vec<u8>> {
    let data: Vec<u8> = converter
        .getattr("convert_tdata_to_session")?
        .call1((tdata,))?
        .extract()?;

    Ok(data)
}

pub fn convert_telethon_to_session(
    converter: Bound<PyModule>,
    telethon_session: &str,
) -> anyhow::Result<Vec<u8>> {
    let data: Vec<u8> = converter
        .getattr("convert_telethon_to_session")?
        .call1((telethon_session,))?
        .extract()?;

    Ok(data)
}

pub fn convert_pyrogram_to_session(
    converter: Bound<PyModule>,
    pyrogram_session: &str,
) -> anyhow::Result<Vec<u8>> {
    let data: Vec<u8> = converter
        .getattr("convert_pyrogram_to_session")?
        .call1((pyrogram_session,))?
        .extract()?;

    Ok(data)
}
pub async fn convert_from_phone_to_session(
    phone: &str,
) -> anyhow::Result<grammers_session::Session> {
    let api_id = env::var("API_ID")?;
    let api_hash = env::var("API_HASH")?;

    let phone = phone.replace("+", "").replace(" ", "");
    let session = grammers_session::Session::load_file_or_create(format!("{phone}.session"))?;

    let client = Client::connect(Config {
        session,
        api_id: api_id.parse().unwrap(),
        api_hash,
        params: Default::default(),
    })
    .await?;

    if !client.is_authorized().await? {
        println!("Signing in...");

        let token = client.request_login_code(&phone).await?;
        let code = prompt("Enter the code you received: ")?;
        let signed_in = client.sign_in(&token, &code).await;
        match signed_in {
            Err(SignInError::PasswordRequired(password_token)) => {
                // Note: this `prompt` method will echo the password in the console.
                //       Real code might want to use a better way to handle this.
                let hint = password_token.hint().unwrap_or("None");
                let prompt_message = format!("Enter the password (hint {}): ", &hint);
                let password = prompt(prompt_message.as_str())?;

                client
                    .check_password(password_token, password.trim())
                    .await?;
            }
            Ok(_) => (),
            Err(e) => panic!("{}", e),
        };
        println!("Signed in!");
        match client.session().save_to_file(format!("{phone}.session")) {
            Ok(_) => {}
            Err(e) => {
                println!(
                    "NOTE: failed to save the session, will sign out when done: {}",
                    e
                );
            }
        }
    }

    let data = client.session().save();
    grammers_session::Session::load(&data).map_err(|e| e.into())
}

#[derive(Clone)]
pub struct Session {
    pub inner: Client,
}

impl Session {
    pub async fn send_message(&self, username: String, message: String) -> anyhow::Result<()> {
        self.inner
            .send_message(
                self.inner.resolve_username(&username).await?.unwrap(),
                message,
            )
            .await?;

        Ok(())
    }

    pub async fn join_channel(&self, username: String) -> anyhow::Result<()> {
        self.inner
            .join_chat(self.inner.resolve_username(&username).await?.unwrap())
            .await?;

        Ok(())
    }

    pub fn telegram(self) -> Client {
        self.inner.clone()
    }

    pub async fn get_init_data(
        &self,
        username: String,
        short_name: String,
    ) -> anyhow::Result<String> {
        let chat = self.inner.resolve_username(&username).await?.unwrap();

        let req = grammers_tl_types::functions::messages::RequestAppWebView {
            start_param: None,
            platform: "android".to_string(),
            write_allowed: true,
            peer: self.inner.get_me().await?.pack().to_input_peer(),
            app: grammers_tl_types::enums::InputBotApp::ShortName(
                grammers_tl_types::types::InputBotAppShortName {
                    bot_id: chat.pack().try_to_input_user().unwrap(),
                    short_name,
                },
            ),
            theme_params: None,
        };

        let response = self.inner.invoke(&req).await?;

        let init_data = match response {
            grammers_tl_types::enums::AppWebViewResult::Url(ref url) => {
                urlencoding::decode(url.url.split("#tgWebAppData=").last().unwrap()).unwrap()
            }
        };

        Ok(init_data.to_string())
    }

    pub async fn connect(credentials: Credentials) -> anyhow::Result<Self> {
        let (api_id, api_hash) = match credentials.source {
            SessionSource::Pyrogram | SessionSource::Telethon => (
                env::var("API_ID").unwrap().parse::<i32>().unwrap(),
                env::var("API_HASH").unwrap(),
            ),
            SessionSource::TDestkop => (2040, String::from("b18441a1ff607e10a989891a5462e627")),
        };

        let session = grammers_session::Session::load(&credentials.data).unwrap();

        Ok(Session {
            inner: Client::connect(Config {
                session,
                api_id,
                api_hash,
                params: InitParams {
                    proxy_url: Some(credentials.proxy.clone()),
                    ..Default::default()
                },
            })
            .await?,
        })
    }
}

#[derive(Clone)]
pub struct Credentials {
    pub data: Vec<u8>,
    pub source: SessionSource,
    pub proxy: String,
}

pub fn create_credentials_from_directories(
    sessions_path: impl Into<PathBuf>,
    proxies_path: impl Into<PathBuf>,
) -> anyhow::Result<Vec<Credentials>> {
    pyo3::prelude::Python::with_gil(|py| {
        let converter =
            PyModule::from_code_bound(py, PYTHON_PROGRAM, "converter.py", "converter.py")?;

        let mut sessions = vec![];

        let mut proxies = String::new();

        File::open(proxies_path.into())?.read_to_string(&mut proxies)?;

        for (session, proxy) in Into::<PathBuf>::into(sessions_path)
            .as_path()
            .read_dir()?
            .zip(proxies.trim().split('\n'))
        {
            let path = session?.path().to_str().unwrap().to_string();

            let source = detect_session_format(converter.clone(), &path)?;

            sessions.push(Credentials {
                source: source.clone(),
                data: match source {
                    SessionSource::Pyrogram => {
                        convert_pyrogram_to_session(converter.clone(), &path)?
                    }
                    SessionSource::TDestkop => convert_tdata_to_session(converter.clone(), &path)?,
                    SessionSource::Telethon => {
                        convert_telethon_to_session(converter.clone(), &path)?
                    }
                },
                proxy: String::from(proxy),
            });
        }

        Result::<Vec<Credentials>, anyhow::Error>::Ok(sessions)
    })
    .map_err(|e| e.into())
}
