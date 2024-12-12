use anyhow::{Ok, Result};
// use hex;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::Client;
// use rsa::{pkcs1v15::Pkcs1v15Encrypt, RsaPublicKey};
// use rsa::{pkcs8::DecodePublicKey, Oaep};
// use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use serde_json::{self, Value};
// use sha2::Sha256;
use std::collections::HashMap;
use std::fs::File;
// use std::io::{self, Read, Write};
use std::io::Read;
use std::path::Path;

#[derive(Serialize, Deserialize)]
#[allow(non_snake_case)]
struct Cookie {
    SESSDATA: String,
    bili_jct: String,
    refresh_token: String,
}

/// 刷新cookie接口逻辑
pub async fn refresh_cookie(client: &Client) -> Result<bool> {
    let path = Path::new("cookie.txt");
    let cookie = read_cookie(path);
    let (code, refresh, timestamp) =
        is_need_refresh(client, &cookie)
            .await
            .unwrap_or((-1, true, String::new()));
    println!(
        "code: {}, refresh: {}, timestamp: {}",
        code, refresh, timestamp
    );
    if code != 0 {
        return Ok(false);
    }
    //let encrypted_hex = correspond_path(&timestamp).unwrap_or("".to_string());
    // match correspond_path(&timestamp) {
    //     Result::Ok(encrypted_hex) => {
    //         println!("encrypted_hex: {}", encrypted_hex);
    //         // let mut input = String::new();
    //         // io::stdin()
    //         //     .read_line(&mut input)
    //         //     .expect("Failed to read line");
    //         get_refresh_csrf(&encrypted_hex, client, &cookie).await?;
    //     }
    //     Err(e) => eprintln!("Error occurred: {}", e),
    // }

    Ok(!refresh)
}

/// 读取cookie文件
fn read_cookie(path: &Path) -> Cookie {
    if path.exists() {
        println!("{:?} exists", path);
        let mut file = File::open(path).unwrap();
        let mut content = String::new();
        file.read_to_string(&mut content).unwrap();
        let cookie: Cookie = serde_json::from_str(&content).unwrap();
        return cookie;
    } else {
        println!("{:?} does not exist", path);
    }
    return Cookie {
        SESSDATA: String::new(),
        bili_jct: String::new(),
        refresh_token: String::new(),
    };
}

/// 创建请求头
fn create_headers(cookie: &Cookie) -> HeaderMap {
    let value = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";
    let mut headers: reqwest::header::HeaderMap = reqwest::header::HeaderMap::new();
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

/// 判断是否需要刷新cookie
async fn is_need_refresh(
    client: &Client,
    cookie: &Cookie,
) -> Result<(i32, bool, String), anyhow::Error> {
    let url = "https://passport.bilibili.com/x/passport-login/web/cookie/info";
    let headers = create_headers(cookie);
    let mut params: HashMap<&str, &str> = HashMap::new();
    params.insert("csrf", cookie.bili_jct.as_str());
    let resp: String = client
        .get(url)
        .headers(headers.clone())
        .query(&params)
        .send()
        .await?
        .text()
        .await?;
    let resp_url: Value = serde_json::from_str(&resp)?;
    //println!("{}", resp_url);
    let code: i32 = resp_url["code"].as_i64().unwrap_or(-1) as i32;
    let fefresh: bool = resp_url["data"]["refresh"].as_bool().unwrap_or(true);
    let timestamp: String = resp_url["data"]["timestamp"]
        .as_i64()
        .unwrap_or(0)
        .to_string();

    Ok((code, fefresh, timestamp))
}

// 加密timestamp
// fn correspond_path(timestamp: &str) -> Result<String> {
//     let pubkey_pem = "-----BEGIN PUBLIC KEY-----
// MIGfMA0GCSqGSIb3DQEBAQUAA4GNADCBiQKBgQDLgd2OAkcGVtoE3ThUREbio0Eg
// Uc/prcajMKXvkCKFCWhJYJcLkcM2DKKcSeFpD/j6Boy538YXnR6VhcuUJOhH2x71
// nzPjfdTcqMz7djHum0qSZA0AyCBDABUqCrfNgCiJ00Ra7GmRj+YCK1NJEuewlb40
// JNrRuoEUXpabUzGB8QIDAQAB
// -----END PUBLIC KEY-----";
//     // ...existing code...
//     let pubkey = RsaPublicKey::from_public_key_pem(pubkey_pem)?;
//     let timestamp = format!("timestamp_{}", timestamp);
//     let padding = Pkcs1v15Encrypt;
//     let mut rng = rand::thread_rng();
//     let encrypted = pubkey.encrypt(&mut rng, padding, timestamp.as_bytes())?;
//     let encrypted_hex = hex::encode(encrypted);
//     Ok(encrypted_hex)
//     // ...existing code...
// }

// async fn get_refresh_csrf(
//     correspond_path: &str,
//     client: &Client,
//     cookie: &Cookie,
// ) -> Result<String, anyhow::Error> {
//     let url = format!("https://www.bilibili.com/correspond/1/{}", correspond_path);
//     //let url = "https://www.baidu.com".to_string();
//     let headers = create_headers(cookie);
//     let resp: String = client
//         .get(&url)
//         .headers(headers)
//         .send()
//         .await?
//         .text()
//         .await?;
//     println!("{:?}", resp);
//     let document = Html::parse_document(&resp);
//     println!("{:?}", document);
//     let selector = Selector::parse("1-name").unwrap();

//     println!("{:?}", selector);
//     // // 查找对应的元素并提取文本
//     // if let Some(element) = document.select(&selector).next() {
//     //     let csrf = element.text().collect::<Vec<_>>().concat();
//     //     println!("csrf: {}", csrf);
//     //     return Ok(csrf);
//     // } else {
//     //     Err(anyhow::anyhow!("CSRF token not found"))
//     // }
//     Ok("".to_string())
// }

// fn correspond_path(timestamp: &str) -> Result<String> {
//     let pubkey_pem = "-----BEGIN PUBLIC KEY-----
// MIGfMA0GCSqGSIb3DQEBAQUAA4GNADCBiQKBgQDLgd2OAkcGVtoE3ThUREbio0Eg
// Uc/prcajMKXvkCKFCWhJYJcLkcM2DKKcSeFpD/j6Boy538YXnR6VhcuUJOhH2x71
// nzPjfdTcqMz7djHum0qSZA0AyCBDABUqCrfNgCiJ00Ra7GmRj+YCK1NJEuewlb40
// JNrRuoEUXpabUzGB8QIDAQAB
// -----END PUBLIC KEY-----";
//     // ...existing code...
//     let pubkey = RsaPublicKey::from_public_key_pem(pubkey_pem)?;
//     let timestamp = format!("timestamp_{}", timestamp);
//     let padding = Oaep::new::<Sha256>();

//     let mut rng = rand::thread_rng();
//     let encrypted = pubkey.encrypt(&mut rng, padding, timestamp.as_bytes())?;
//     let encrypted_hex = hex::encode(encrypted);
//     Ok(encrypted_hex)
//     // ...existing code...
// }
