use anyhow::Result;
use qrcode::render::svg;
use qrcode::QrCode;
use reqwest::{header::HeaderValue, Client};
use resvg::tiny_skia::Pixmap;
use resvg::usvg::{Options, Transform, Tree};
use serde_json::Value;
use std::collections::HashMap;
use std::io::Write;
use std::process::Command;
use urlencoding::encode;

/// 渲染SVG到PNG
fn render_svg_to_png(svg_data: &str, output_path: &str) -> Result<()> {
    let options = Options::default();
    let tree = Tree::from_str(svg_data, &options)?;
    let size = tree.size();
    let width = size.width() as u32;
    let height = size.height() as u32;
    let mut pixmap = Pixmap::new(width, height)
        .ok_or("Failed to create pixmap")
        .unwrap();
    resvg::render(&tree, Transform::default(), &mut pixmap.as_mut());
    pixmap.save_png(output_path)?;

    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(&["/C", "start", output_path])
            .spawn()?;
    }

    Ok(())
}

/// 获取二维码接口数据
async fn apply_qrcode(client: &Client) -> Result<String, reqwest::Error> {
    let url = "https://passport.bilibili.com/x/passport-login/web/qrcode/generate";
    let resp = client.get(url).send().await?.text().await?;
    Ok(resp)
}

/// 解析二维码接口数据
fn get_url_and_key(response: &str) -> (&str, &str) {
    let url = &response[47..193]; //切片
    let key = &response[209..241];
    return (url, key);
}

/// 显示二维码
fn show_qrcode(url: &str) -> Result<()> {
    let code = QrCode::new(url.as_bytes()).unwrap();
    let rendered = code
        .render()
        .dark_color(svg::Color("#000000"))
        .light_color(svg::Color("#FFFFFF"))
        .build();
    render_svg_to_png(&rendered, "output.png")?;
    Ok(())
}

/// 轮询二维码登录状态
async fn qrcode_pull(client: &Client, qrcode_key: &str) -> Result<bool, reqwest::Error> {
    let mut flag: bool = false;
    let url = "https://passport.bilibili.com/x/passport-login/web/qrcode/poll";
    let value = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";
    let mut headers: reqwest::header::HeaderMap = reqwest::header::HeaderMap::new();
    headers.insert("User-Agent", HeaderValue::from_static(value));
    headers.insert(
        "Referer",
        HeaderValue::from_static("https://passport.bilibili.com"),
    );
    let mut params: HashMap<&str, &str> = HashMap::new();
    params.insert("qrcode_key", qrcode_key);
    let cookie: Option<String>;
    let mut count = 0;
    loop {
        let resp: String = client
            .get(url)
            .headers(headers.clone())
            .query(&params)
            .send()
            .await?
            .text()
            .await?;
        //println!("{}", resp);
        let (code1, url, refresh_token, code2, message) = wait_for_login(resp);
        if code1 == 0 {
            if code2 == 0 {
                //登录成功
                std::fs::remove_file("output.png").unwrap();
                let result = format!("{}&refresh_token={}", url, refresh_token);
                cookie = Some(result);
                flag = true;
                break;
            } else {
                println!("Code: {}, Message: {}", code2, message);
            }
        } else {
            println!("ip is baned, please change ip");
            cookie = None;
            break;
        }
        std::thread::sleep(std::time::Duration::from_secs_f64(3.0));
        count += 3;
        if count >= 180 {
            // 3分钟超时
            std::fs::remove_file("output.png").unwrap();
            println!("Timeout");
            cookie = None;
            break;
        }
    }
    if let Some(cookie) = &cookie {
        match save_cookie(cookie.to_string()) {
            Ok(_) => {
                println!("Cookie saved successfully");
                flag = true;
            }
            Err(e) => eprintln!("Error occurred: {}", e),
        };
    } else {
        eprintln!("Error: Cookie is not initialized");
    }
    Ok(flag)
}

/// 保存cookie到文件
fn save_cookie(cookie: String) -> Result<bool> {
    let parts: Vec<&str> = cookie.split('?').collect();
    let params: Vec<&str> = parts[1].split('&').collect();
    let mut map: std::collections::HashMap<&str, String> = std::collections::HashMap::new();
    for param in params.iter() {
        let kv: Vec<&str> = param.split('=').collect();
        let key = kv[0];
        let value = encode(kv[1]).into_owned().to_string(); // url编码
        map.insert(key, value);
    }
    let mut file = std::fs::File::create("cookie.txt")?;
    let serialized_map = serde_json::to_string(&map)?;
    file.write_all(serialized_map.as_bytes())?;
    Ok(true)
}

fn wait_for_login(response: String) -> (i32, String, String, i32, String) {
    let parsed: Value = serde_json::from_str(&response).unwrap();
    let code1 = parsed["code"].as_i64().unwrap() as i32;
    let code2 = parsed["data"]["code"].as_i64().unwrap() as i32;
    let url = parsed["data"]["url"].as_str().unwrap();
    let refresh_token = parsed["data"]["refresh_token"].as_str().unwrap();
    let message = parsed["data"]["message"].as_str().unwrap();

    return (
        code1,
        url.to_string(),
        refresh_token.to_string(),
        code2,
        message.to_string(),
    );
}

/// 登录二维码接口逻辑
pub async fn login_qrcode(client: &Client) -> bool {
    let qrcode_key: Option<String>;
    let response: Option<String> = match apply_qrcode(&client).await {
        Ok(response) => Some(response),
        Err(e) => {
            eprintln!("Error occurred: {}", e);
            None
        }
    };
    if let Some(response) = response {
        let (url, key) = get_url_and_key(&response);
        let decoded_url = url.replace(r"\u0026", "&");
        //println!("QR Code URL: {}", decoded_url);
        //println!("QR Code key: {}", key);
        qrcode_key = Some(key.to_string().clone());
        match show_qrcode(&decoded_url) {
            Err(e) => eprintln!("Error occurred: {}", e),
            Ok(_) => println!("QR Code displayed successfully"),
        };
    } else {
        println!("Error occurred: {}", "No response");
        return false;
    }

    if let Some(qrcode_key) = qrcode_key {
        match qrcode_pull(&client, &qrcode_key).await {
            Ok(flag) => {
                return flag;
            }
            Err(e) => {
                eprintln!("Error occurred: {}", e);
                return false;
            }
        }
    } else {
        eprintln!("Error: QR Code key is not initialized");
        return false;
    }
}
