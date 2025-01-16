mod qrcode_login;
use crate::qrcode_login::login_qrcode;
use anyhow::{Context, Result};
use core::f32;
use reqwest::Client;
use std::sync::Arc;
use std::{io, result};
use tokio::sync::Mutex;
mod down_bangumi;
mod down_bv;
mod init_;
mod refresh_cookie;
mod wbi;
use eframe::egui;
use eframe::egui::{ComboBox, FontDefinitions, FontFamily, ProgressBar, Vec2};
mod resolution;

#[tokio::main]
async fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        ..Default::default()
    };

    eframe::run_native(
        "Bili Downloader",
        options,
        Box::new(|_cc| Ok(Box::new(MyApp::new(_cc)) as Box<dyn eframe::App>)),
    )
}

impl MyApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        load_fonts(&_cc.egui_ctx);
        Self {
            url: String::new(),
            selected_resolution: String::new(),
            resolutions: vec![
                "HDR".to_string(),
                "4K".to_string(),
                "1080P+".to_string(),
                "1080P60".to_string(),
                "1080P".to_string(),
                "720P".to_string(),
                "480P".to_string(),
                "360P".to_string(),
            ],
            info: String::new(),
            pic: false,
            mutex_login: Arc::new(Mutex::new(false)),
            mutex_info: Arc::new(Mutex::new(String::new())),
        }
    }
    fn update_info(&mut self, info: String) {
        self.info = info;
    }
    fn update_pic(&mut self, pic: bool) {
        self.pic = pic;
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
        let mutex_info = Arc::clone(&self.mutex_info);
        let rsl = self.selected_resolution.clone();
        tokio::spawn(async move {
            match init_::get_title_pic(&video).await {
                Ok((t, _)) => {
                    let mut lock_t = mutex_info.lock().await;
                    *lock_t = t;
                }
                Err(e) => eprintln!("Error occurred: {}", e),
            }

            let result = init_::choose_download_method(&video, &rsl).await;
            match result {
                Ok(title) => {
                    println!("Download completed for {}", title);
                }
                Err(e) => eprintln!("Error occurred: {}", e),
            }
        });
        //let x = &self.mutex_info;
        //println!("aaa{:?}", x);
    }
}

#[derive(Default)]
struct MyApp {
    url: String,
    selected_resolution: String,
    resolutions: Vec<String>,
    info: String,
    pic: bool,
    mutex_login: Arc<Mutex<bool>>,
    mutex_info: Arc<Mutex<String>>,
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let available_size = ui.available_size();

            // 动态计算各个部件的大小
            let button_size = Vec2::new(available_size.x * 0.2, available_size.y * 0.05);
            let text_edit_width = available_size.x * 0.8;
            let text_edit_height = available_size.y * 0.1;
            let image_size = Vec2::new(available_size.x * 0.2, available_size.x * 0.2);
            let info_width = available_size.x * 0.6;
            let info_height = available_size.y * 0.3;

            ui.horizontal(|ui| {
                ui.heading("bilibili视频下载器");
                if ui
                    .add_sized(button_size, egui::Button::new("登录"))
                    .clicked()
                {
                    self.login();
                }
                if ui
                    .add_sized(button_size, egui::Button::new("登出"))
                    .clicked()
                {
                    println!("登出按钮点击");
                    match std::fs::remove_file("load") {
                        Ok(_) => println!("delogin successful"),
                        Err(e) => eprintln!("Error occurred: {}", e),
                    }
                }
            });

            ui.separator();

            ui.horizontal(|ui| {
                ui.label("视频地址输入:");
                ui.add_sized(
                    Vec2::new(text_edit_width, text_edit_height),
                    egui::TextEdit::multiline(&mut self.url),
                );
            });

            ui.horizontal(|ui| {
                if ui
                    .add_sized(button_size, egui::Button::new("下载"))
                    .clicked()
                {
                    println!("分辨率:{}", self.selected_resolution);
                    self.handle_down();
                }
            });

            ui.horizontal(|ui| {
                ui.label("分辨率");
                egui::ComboBox::new(egui::Id::new("resolution_select"), "")
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

            ui.separator();

            ui.horizontal(|ui| {
                let info_text = {
                    let info = self.mutex_info.clone();
                    tokio::task::block_in_place(|| {
                        let lock = futures::executor::block_on(info.lock());
                        lock.clone()
                    })
                };
                self.update_info(info_text.clone());
                if self.info.is_empty() {
                    self.update_pic(false);
                } else {
                    self.update_pic(true);
                }
                ui.vertical(|ui| {
                    ui.label("封面:");
                    //println!("pic:{}", self.pic);
                    if self.pic {
                        ui.add(
                            egui::Image::new(egui::include_image!("../pic.png"))
                                .max_width(200.0)
                                .rounding(10.0),
                        );
                    } else {
                        ui.label("no picture");
                    }
                });
                ui.vertical(|ui| {
                    ui.label("标题:");
                    ui.add_sized(
                        Vec2::new(info_width, info_height),
                        egui::TextEdit::multiline(&mut self.info)
                            .desired_rows(3)
                            .desired_width(f32::INFINITY)
                            .interactive(false),
                    );
                });
            });

            ui.separator();

            ui.label("process:");
            // ui.add(ProgressBar::new(self.progress).show_percentage());
        });
    }
}

fn load_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "my_font".to_owned(),
        egui::FontData::from_static(include_bytes!("D:\\精简版微软雅黑TTF.ttf")),
    );
    fonts
        .families
        .get_mut(&egui::FontFamily::Proportional)
        .unwrap()
        .insert(0, "my_font".to_owned());
    fonts
        .families
        .get_mut(&egui::FontFamily::Monospace)
        .unwrap()
        .push("my_font".to_owned());
    ctx.set_fonts(fonts);
}

async fn login() {
    let client: Client = reqwest::Client::new();
    if login_qrcode(&client).await {
        println!("Login successful");
    } else {
        println!("Login failed");
    }
}

// async fn initiation() {
//     login().await;
//     loop {
//         let mut url = String::new();
//         println!("\nPlease input the url of the bilibili to download, or 'exit' to exit:");
//         loop {
//             url.clear();
//             io::stdin()
//                 .read_line(&mut url)
//                 .expect("Failed to read line");
//             if url != "\r\n" {
//                 break;
//             }
//         }

//         if url.trim() == "exit" {
//             break;
//         }

//         let video = match init_::get_epid_season(&url) {
//             Ok(v) => v,
//             Err(e) => {
//                 eprintln!("Error occurred: {}", e);
//                 continue;
//             }
//         };
//         println!("{:?}", video);
//         if let Err(e) = init_::choose_download_method(&video).await {
//             eprintln!("Error occurred: {}", e);
//         }
//     }
// }

// /// 调用二维码登录函数
// async fn login_() {
//     println!("waiting for login...");
//     let client: Client = reqwest::Client::new();
//     match refresh_cookie::refresh_cookie(&client).await {
//         Ok(flag) => {
//             if flag {
//                 println!("dont need to refresh cookie\nLogin successful");
//             } else {
//                 println!("cookie is out of date or havent logined, please login again");
//                 if login_qrcode(&client).await {
//                     println!("Login successful");
//                 } else {
//                     println!("Login failed");
//                 }
//             }
//         }
//         Err(e) => eprintln!("Error occurred: {}", e),
//     }
// }
