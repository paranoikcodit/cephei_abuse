use std::{env, path::Path, sync::Arc};

use bitcode::{Decode, Encode};
use cephei::Cephei;
use dialoguer::theme::ColorfulTheme;
use futures_util::Future;
use grammers_client::{Client, Config, InitParams};
use grammers_tl_types::{Cursor, Deserializable};
use pyo3::types::PyModule;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use telegram::{
    create_credentials_from_directories, detect_session_format, Session, SessionSource,
    PYTHON_PROGRAM,
};
use tokio::{
    fs::File,
    io::AsyncReadExt,
    sync::{Mutex, Semaphore},
    task::JoinError,
};

mod cephei;
mod telegram;

pub async fn semaphore_datas<T, F, R, R1>(
    max_workers: usize,
    datas: Vec<T>,
    f: F,
) -> Vec<std::result::Result<(T, R1), JoinError>>
where
    T: Clone + Sync + Send + 'static,
    F: Fn(T) -> R + Send + Sync + 'static, // Added + 'static here
    R1: Sync + Send + 'static,
    R: Future<Output = R1> + Send,
{
    let semaphore = Arc::new(Semaphore::new(max_workers));
    let f = Arc::new(Mutex::new(f));

    let handles: Vec<_> = datas
        .into_iter()
        .map(|data| {
            let semaphore = semaphore.clone();
            let f = f.clone();

            tokio::spawn(async move {
                let f = f.lock().await;

                let permit = semaphore.acquire().await.unwrap();
                let _result = f(data.clone()).await;
                drop(permit);

                (data, _result)
            })
        })
        .collect();

    futures_util::future::join_all(handles).await
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv()?;

    let sessions = create_credentials_from_directories("./sessions", "./proxies.txt")?;

    /*

    {nickname: "dsaodkas", invite_code: null, image: null};
     */

    let sessions: Vec<_> =
        futures_util::future::join_all(sessions.iter().cloned().map(|s| async move {
            let session = Session::connect(s.clone()).await?;
            let cephei = Cephei::auth(session.clone(), s.proxy).await?;

            session
                .send_message("Cephei_fi_bot".to_string(), "/start".to_string())
                .await?;

            Result::<(Session, Cephei), anyhow::Error>::Ok((session, cephei))
        }))
        .await
        .iter()
        .flatten()
        .cloned()
        .collect();

    println!("{}", sessions.len());

    loop {
        let sessions = sessions.clone();

        let result = dialoguer::Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select in menu:")
            .default(0)
            .items(&[
                "Join to cephei channels",
                "Register an accounts",
                "Start farming a points",
                "Claim points",
                "Complete tasks",
            ])
            .interact()?;

        let results = match result {
            0 => {
                semaphore_datas(5, sessions, |(session, _)| async move {
                    session.join_channel("cephei_fi".to_string()).await?;
                    session.join_channel("cephei_fi_cis".to_string()).await?;

                    Result::<(), anyhow::Error>::Ok(())
                })
                .await
            }
            1 => {
                semaphore_datas(5, sessions, |(session, cephei)| async move {
                    let me = session.telegram().get_me().await?;

                    cephei
                        .register(
                            me.first_name().to_string(),
                            Some(std::env::var("REF_CODE")?),
                        )
                        .await?;

                    Result::<(), anyhow::Error>::Ok(())
                })
                .await
            }
            2 => {
                semaphore_datas(5, sessions, |(session, cephei)| async move {
                    cephei.start_farming().await?;

                    Result::<(), anyhow::Error>::Ok(())
                })
                .await
            }
            3 => {
                semaphore_datas(5, sessions, |(session, cephei)| async move {
                    cephei.claim_farming().await?;

                    Result::<(), anyhow::Error>::Ok(())
                })
                .await
            }
            4 => {
                semaphore_datas(5, sessions, |(session, cephei)| async move {
                    let response = cephei.get_tasks().await?["content"].clone();

                    for task in response.as_array().unwrap() {
                        cephei.check_task(task["id"].as_str().unwrap()).await?;
                        cephei.claim_task(task["id"].as_str().unwrap()).await?;
                    }

                    Result::<(), anyhow::Error>::Ok(())
                })
                .await
            }
            _ => {
                println!("Ты еблан?");

                return Ok(());
            }
        };

        for result in results {
            let ((session, _), result) = result?;

            if let Err(err) = result {
                println!(
                    "Failed to execute function with {}: {:#?}",
                    session.inner.get_me().await?.first_name(),
                    err
                );
            }
        }
    }
}
