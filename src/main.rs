mod qrcode_login;
use crate::qrcode_login::login_qrcode;
use reqwest::Client;
use std::io;
mod refresh_cookie;

#[tokio::main]
async fn main() {
    // 调用二维码登录函数
    let client: Client = reqwest::Client::new();

    // if login_qrcode(&client).await {
    //     println!("Login successful");
    // } else {
    //     println!("Login failed");
    // }

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

    // 阻止终端自动关闭
    println!("\nPress Enter to exit...");
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .expect("Failed to read line");
}
