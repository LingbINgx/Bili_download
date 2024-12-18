mod qrcode_login;
use crate::qrcode_login::login_qrcode;
use anyhow::{Context, Result};
use reqwest::Client;
use std::io;
mod down_bangumi;
mod down_bv;
mod refresh_cookie;
mod wbi;

#[derive(Debug)]
struct Video {
    ep_id: String,
    season_id: String,
    bv_id: String,
}

#[tokio::main]
async fn main() {
    init().await;
    loop {
        let mut url = String::new();
        println!(
            "Please input the url of the bangumi you want to download, or input 'exit' to exit:"
        );
        loop {
            url.clear();
            io::stdin()
                .read_line(&mut url)
                .expect("Failed to read line");
            if url != "\r\n" {
                break;
            }
        }

        if url.trim() == "exit" {
            break;
        }

        let video = match get_epid_season(&url) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Error occurred: {}", e);
                continue;
            }
        };
        println!("{:?}", video);
        if let Err(e) = choose_download_method(&video).await {
            eprintln!("Error occurred: {}", e);
        }
    }
}

/// 调用二维码登录函数
async fn init() {
    println!("waiting for login...");
    let client: Client = reqwest::Client::new();
    match refresh_cookie::refresh_cookie(&client).await {
        Ok(flag) => {
            if flag {
                println!("dont need to refresh cookie\nLogin successful");
            } else {
                println!("cookie is out of date or havent logined, please login again");
                if login_qrcode(&client).await {
                    println!("Login successful");
                } else {
                    println!("Login failed");
                }
            }
        }
        Err(e) => eprintln!("Error occurred: {}", e),
    }
}

/// 获取网址中的epid/seasonid/bv
fn get_epid_season(url: &str) -> Result<Video> {
    let url = url.trim();
    let parts: Vec<&str> = url.split('?').collect();
    let path_parts: Vec<&str> = parts
        .get(0)
        .context("URL does not contain a valid path")?
        .split('/')
        .collect();
    let id = path_parts
        .iter()
        .rev()
        .find(|&&x| !x.is_empty())
        .context("Failed to extract the last part of the URL path")?;
    if id.starts_with("ep") {
        let ep_id = id.trim_start_matches("ep").to_string();
        Ok(Video {
            ep_id,
            season_id: String::new(),
            bv_id: String::new(),
        })
    } else if id.starts_with("ss") {
        let season_id = id.trim_start_matches("ss").to_string();
        Ok(Video {
            ep_id: String::new(),
            season_id,
            bv_id: String::new(),
        })
    } else if id.starts_with("BV") {
        let bv_id = id.to_string();
        Ok(Video {
            ep_id: String::new(),
            season_id: String::new(),
            bv_id,
        })
    } else {
        Err(anyhow::anyhow!(
            "URL does not contain valid episode ,season ID or BV ID"
        ))
    }
}

async fn choose_download_method(video: &Video) -> Result<()> {
    if !video.ep_id.is_empty() || !video.season_id.is_empty() {
        down_bangumi::down_main((&video.ep_id, &video.season_id)).await?;
    } else if !video.bv_id.is_empty() {
        down_bv::down_main(&video.bv_id).await?;
    } else {
        Err(anyhow::anyhow!("No valid video ID found"))?;
    }
    Ok(())
}
