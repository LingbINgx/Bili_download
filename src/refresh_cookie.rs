use anyhow::{Ok, Result};
use chrono::{DateTime, TimeZone, Utc};
use hex;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::Client;
use rsa::RsaPublicKey;
use rsa::{pkcs8::DecodePublicKey, Oaep};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use serde_json::{self, Value};
use sha2::Sha256;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::process::Command;

#[derive(Serialize, Deserialize)]
#[allow(non_snake_case)]
#[derive(Debug)]
pub struct Cookies {
    pub SESSDATA: String,
    pub bili_jct: String,
    pub refresh_token: String,
}

// /// 刷新cookie接口逻辑
// pub async fn refresh_cookie(client: &Client) -> Result<bool> {
//     let path = Path::new("load");
//     let cookie = read_cookie(path);
//     let (code, refresh, timestamp) =
//         is_need_refresh(client, &cookie)
//             .await
//             .unwrap_or((-1, true, String::new()));
//     let date_time = convert_timestamp_to_date(timestamp.parse::<i64>().unwrap_or(0));
//     println!(
//         "code: {}, refresh: {}, timestamp: {}",
//         code, refresh, date_time
//     );
//     if code != 0 {
//         return Ok(false);
//     }
//     //let encrypted_hex = correspond_path(&timestamp).unwrap_or("".to_string());
//     // match correspond_path(&timestamp) {
//     //     Result::Ok(encrypted_hex) => {
//     //         println!("encrypted_hex: {}", encrypted_hex);
//     //         // let mut input = String::new();
//     //         // io::stdin()
//     //         //     .read_line(&mut input)
//     //         //     .expect("Failed to read line");
//     //         get_refresh_csrf(&encrypted_hex, client, &cookie).await?;
//     //     }
//     //     Err(e) => eprintln!("Error occurred: {}", e),
//     // }

//     Ok(!refresh)
// }

// /// 将时间戳转换为日期时间 +8
// fn convert_timestamp_to_date(timestamp: i64) -> DateTime<Utc> {
//     let timestamp_in_seconds = timestamp / 1000 + 8 * 3600;
//     Utc.timestamp_opt(timestamp_in_seconds, (timestamp % 1000) as u32 * 1_000_000)
//         .single()
//         .expect("Invalid timestamp")
// }

/// 读取cookie文件
pub fn read_cookie(path: &Path) -> Cookies {
    if path.exists() {
        //println!("{:?} exists", path);
        let mut file = File::open(path).unwrap();
        let mut content = String::new();
        file.read_to_string(&mut content).unwrap();
        let cookie: Cookies = serde_json::from_str(&content).unwrap();
        return cookie;
    } else {
        println!("{:?} does not exist", path);
    }
    return Cookies {
        SESSDATA: String::new(),
        bili_jct: String::new(),
        refresh_token: String::new(),
    };
}

/// 创建请求头
pub fn create_headers(cookie: &Cookies) -> HeaderMap {
    let value = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";
    let mut headers: HeaderMap = HeaderMap::new();
    headers.insert("User-Agent", HeaderValue::from_static(value));
    headers.insert(
        "Referer",
        HeaderValue::from_static("https://www.bilibili.com"),
    );
    headers.insert(
        "Cookie",
        HeaderValue::from_str(format!("SESSDATA={}", cookie.SESSDATA).as_str()).unwrap(),
    );
    return headers;
}

// /// 判断是否需要刷新cookie
// async fn is_need_refresh(
//     client: &Client,
//     cookie: &Cookies,
// ) -> Result<(i32, bool, String), anyhow::Error> {
//     let url = "https://passport.bilibili.com/x/passport-login/web/cookie/info";
//     let headers = create_headers(cookie);
//     let mut params: HashMap<&str, &str> = HashMap::new();
//     params.insert("csrf", cookie.bili_jct.as_str());
//     let resp: String = client
//         .get(url)
//         .headers(headers.clone())
//         .query(&params)
//         .send()
//         .await?
//         .text()
//         .await?;
//     let resp_url: Value = serde_json::from_str(&resp)?;
//     //println!("{}", resp_url);
//     let code: i32 = resp_url["code"].as_i64().unwrap_or(-1) as i32;
//     let fefresh: bool = resp_url["data"]["refresh"].as_bool().unwrap_or(true);
//     let timestamp: String = resp_url["data"]["timestamp"]
//         .as_i64()
//         .unwrap_or(0)
//         .to_string();

//     Ok((code, fefresh, timestamp))
// }

/*
async fn get_refresh_csrf(
    correspond_path: &str,
    client: &Client,
    cookie: &Cookies,
) -> Result<String, anyhow::Error> {
    Ok("".to_string())
}

fn correspond_path(timestamp: &str) -> Result<String> {
    let pubkey_pem = r#"-----BEGIN PUBLIC KEY-----
MIGfMA0GCSqGSIb3DQEBAQUAA4GNADCBiQKBgQDLgd2OAkcGVtoE3ThUREbio0Eg
Uc/prcajMKXvkCKFCWhJYJcLkcM2DKKcSeFpD/j6Boy538YXnR6VhcuUJOhH2x71
nzPjfdTcqMz7djHum0qSZA0AyCBDABUqCrfNgCiJ00Ra7GmRj+YCK1NJEuewlb40
JNrRuoEUXpabUzGB8QIDAQAB
-----END PUBLIC KEY-----"#;
    let pubkey = RsaPublicKey::from_public_key_pem(pubkey_pem)?;
    let timestamp = format!("refresh_{}", timestamp);
    let padding = Oaep::new::<Sha256>();

    let mut rng = rand::thread_rng();
    let encrypted = pubkey.encrypt(&mut rng, padding, timestamp.as_bytes())?;
    let encrypted_hex = hex::encode(encrypted);
    Ok(encrypted_hex)
}

#[tokio::test]
async fn test_csrf() {
    let client: Client = reqwest::Client::builder()
        .cookie_store(true)
        .build()
        .unwrap();

    let path = Path::new("load");
    let cookie = read_cookie(path);
    let (x, y, timestamp) = is_need_refresh(&client, &cookie).await.unwrap();
    //let timestamp = "1734095039907";
    let encrypted_hex = correspond_path(&timestamp).unwrap();
    println!("\n{}", encrypted_hex);

    let csrf = get_refresh_csrf(&encrypted_hex, &client, &cookie)
        .await
        .unwrap();
    println!("\n{}", csrf);
}

#[test]
fn test_correspond_path() {
    let timestamp = "1734097847297";
    let encrypted_hex = correspond_path(&timestamp).unwrap();
    println!("\n{}", encrypted_hex);
}
*/
