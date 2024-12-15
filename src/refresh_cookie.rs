use anyhow::{Ok, Result};
//use curl::easy::{Easy, List};
//use hex;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::Client;
//use rsa::RsaPublicKey;
//use rsa::{pkcs8::DecodePublicKey, Oaep};
//use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use serde_json::{self, Value};
//use sha2::Sha256;
use std::collections::HashMap;
use std::fs::File;
//use std::io::{self, Read, Write};
use std::io::Read;
use std::path::Path;
//use std::process::Command;

#[derive(Serialize, Deserialize)]
#[allow(non_snake_case)]
#[derive(Debug)]
pub struct Cookies {
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

/// 判断是否需要刷新cookie
async fn is_need_refresh(
    client: &Client,
    cookie: &Cookies,
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

/*
async fn get_refresh_csrf(
    correspond_path: &str,
    client: &Client,
    cookie: &Cookies,
) -> Result<String, anyhow::Error> {
    let url: String = format!("https://www.bilibili.com/correspond/1/{}", correspond_path);
    //let url = "https://www.bilibili.com".to_string();
    println!("{}\n", url);
    let headers = create_headers(cookie);
    let mut params: HashMap<&str, &str> = HashMap::new();
    params.insert("csrf", cookie.bili_jct.as_str());

    let resp = client
        .get(&url)
        .headers(headers)
        //.query(&params)
        .send()
        //.await?
        //.text()
        .await?;

    println!("{:?}\n", resp);
    let resp = resp.text().await?;
    println!("{:?}\n", resp);
    let document = Html::parse_document(&resp);
    println!("{:?}\n", document);
    let selector = Selector::parse("#1-name").expect("Invalid CSS selector");

    //println!("1-name: {:?}", selector);
    // 查找对应的元素并提取文本
    if let Some(element) = document.select(&selector).next() {
        let csrf = element.text().collect::<Vec<_>>().concat();
        println!("csrf: {}", csrf);
        return Ok(csrf);
    } else {
        return Err(anyhow::anyhow!("CSRF token not found"));
    }
    Ok("".to_string())
    -------------------------------------------
    let output = Command::new("curl")
        .arg("-G")
        .arg(&url)
        .arg("-b")
        .arg(format!("SESSDATA={}", &cookie.SESSDATA))
        .output()
        .expect("Failed to execute curl command");

    if !output.status.success() {
        eprintln!("Error: {}", String::from_utf8_lossy(&output.stderr));
    }

    // 打印响应内容
    println!("Response: {}\n", String::from_utf8_lossy(&output.stdout));
    //Ok("".to_string())

    //-------------------------------------------
    let mut easy = Easy::new();

    // 设置 URL
    easy.url(&url)?;

    // 设置请求头（例如设置 Cookie）
    let mut headers = List::new();
    headers.append(&format!("Cookie: SESSDATA={}", cookie.SESSDATA))?;
    easy.http_headers(headers)?;

    // 执行请求
    let mut response_data = Vec::new();
    {
        let mut transfer = easy.transfer();
        transfer.write_function(|data| {
            response_data.extend_from_slice(data);
            Result::Ok(data.len())
        })?;
        transfer.perform()?;
    }

    // 打印响应内容
    let response_text = String::from_utf8_lossy(&response_data);
    println!("\nResponse: {}", response_text);

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

    let path = Path::new("cookie.txt");
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
