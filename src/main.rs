mod qrcode_login;
use crate::qrcode_login::login_qrcode;
use reqwest::Client;
use std::io;
mod down;
mod refresh_cookie;

async fn init() {
    // 调用二维码登录函数
    let client: Client = reqwest::Client::new();

    match refresh_cookie::refresh_cookie(&client).await {
        Ok(flag) => {
            if flag {
                println!("dont need to refresh cookie");
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

        if let Err(e) = down::down_main(&url).await {
            eprintln!("Download failed: {}", e);
        }

        // 阻止终端自动关闭
        // println!("\nPress Enter to exit...");
        // let mut input = String::new();
        // io::stdin()
        //     .read_line(&mut input)
        //     .expect("Failed to read line");
    }
}
