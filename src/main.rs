mod qrcode_login;
use crate::qrcode_login::login_qrcode;
use anyhow::{Context, Result};
use core::f32;
use reqwest::Client;
use std::sync::Arc;
use std::{io, result};
use tokio::runtime::{self, Runtime};
use tokio::sync::Mutex;
mod down_bangumi;
mod down_bv;
mod init_;
mod refresh_cookie;
mod wbi;
use eframe::egui;
use eframe::egui::{ComboBox, ProgressBar};

#[tokio::main]
async fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Bili Downloader",
        options,
        Box::new(|_cc| Ok(Box::new(MyApp::new()) as Box<dyn eframe::App>)),
    )
}

impl MyApp {
    fn new() -> Self {
        Self {
            url: String::new(),
            selected_resolution: String::new(),
            resolutions: vec![
                "HDR".to_string(),
                "4K".to_string(),
                "1080p+".to_string(),
                "1080p".to_string(),
                "720p".to_string(),
                "480p".to_string(),
            ],
            info: String::new(),
            mutex_login: Arc::new(Mutex::new(false)),
        }
    }
    fn load_image(&mut self, title: &str, url: &str) {
        //let client = Client::new();
        self.info = title.to_string();
    }
    fn login(&mut self) {
        println!("登录按钮点击{:?}", self.mutex_login);
        let mutex_login = Arc::clone(&self.mutex_login);
        tokio::spawn(async move {
            let mut mutex_login = match mutex_login.try_lock() {
                Ok(guard) => guard,
                Err(_) => {
                    println!("Failed to acquire login lock");
                    return;
                }
            };
            if !*mutex_login {
                *mutex_login = true;
                login().await;
                *mutex_login = false;
            }
        });
    }
    fn handle_down(&mut self) {
        println!("下载按钮点击");
        let url = self.url.clone();
        let video = match init_::get_epid_season(&url) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Error occurred: {}", e);
                return;
            }
        };
        println!("{:?}", video);
        let title = Arc::new(Mutex::new(String::new()));
        let pic = Arc::new(Mutex::new(String::new()));
        tokio::spawn(async move {
            match init_::get_title_pic(&video).await {
                Ok((t, p)) => {
                    let mut title_lock = title.lock().await;
                    let mut pic_lock = pic.lock().await;
                    *title_lock = t;
                    *pic_lock = p;
                }
                Err(e) => eprintln!("Error occurred: {}", e),
            }

            let result = init_::choose_download_method(&video).await;
            match result {
                Ok(title) => {
                    println!("Download completed for {}", title);
                }
                Err(e) => eprintln!("Error occurred: {}", e),
            }
        });
    }
}

#[derive(Default)]
struct MyApp {
    url: String,
    selected_resolution: String,
    resolutions: Vec<String>,
    info: String,
    mutex_login: Arc<Mutex<bool>>,
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Title");
                if ui.button("Login").clicked() {
                    self.login();
                }
                if ui.button("delogin").clicked() {
                    println!("登出按钮点击");
                    match std::fs::remove_file("load") {
                        Ok(_) => println!("delogin successful"),
                        Err(e) => eprintln!("Error occurred: {}", e),
                    }
                }
            });

            ui.separator(); // 分隔线

            // URL 输入
            ui.horizontal(|ui| {
                ui.label("URL in:");
                ui.text_edit_multiline(&mut self.url);
            });

            ui.horizontal(|ui| {
                if ui.button("Download").clicked() {
                    self.handle_down();
                }
            });

            ui.horizontal(|ui| {
                ui.label("Resolution");
                ComboBox::new(egui::Id::new("resolution_select"), "")
                    .selected_text(&self.selected_resolution)
                    .show_ui(ui, |ui| {
                        for resolution in &self.resolutions {
                            ui.selectable_value(
                                &mut self.selected_resolution,
                                resolution.clone(),
                                resolution,
                            );
                        }
                    });
            });

            ui.separator(); // 分隔线

            // 图片预览和信息区域
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label("picture:");
                    // 这里你可以使用一个自定义控件来显示图片预览
                    ui.add_sized(
                        [100.0, 100.0],
                        egui::ImageButton::new((
                            egui::TextureId::default(),
                            egui::Vec2::new(100.0, 100.0),
                        )),
                    );
                });

                ui.vertical(|ui| {
                    ui.label("detail:");
                    ui.text_edit_multiline(&mut self.info);
                    ui.add(
                        egui::TextEdit::multiline(&mut self.info)
                            .desired_rows(5)
                            .desired_width(f32::INFINITY)
                            .interactive(false),
                    );
                });
            });

            ui.separator(); // 分隔线

            // 进度条
            ui.label("process:");
            //ui.add(ProgressBar::new(self.progress).show_percentage());
        });
    }
}

async fn initiation() {
    login().await;
    loop {
        let mut url = String::new();
        println!("\nPlease input the url of the bilibili to download, or 'exit' to exit:");
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

        let video = match init_::get_epid_season(&url) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Error occurred: {}", e);
                continue;
            }
        };
        println!("{:?}", video);
        if let Err(e) = init_::choose_download_method(&video).await {
            eprintln!("Error occurred: {}", e);
        }
    }
}

/// 调用二维码登录函数
async fn login_() {
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

async fn login() {
    let client: Client = reqwest::Client::new();
    if login_qrcode(&client).await {
        println!("Login successful");
    } else {
        println!("Login failed");
    }
}
