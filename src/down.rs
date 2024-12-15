use anyhow::{Ok, Result};
use hex::decode;
use rand::seq::index;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Client, Response};
use serde::{Deserialize, Serialize};
use serde_json::{self, Value};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use tokio::process::Command;
//use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::join;

use crate::refresh_cookie::{create_headers, read_cookie, Cookies};

async fn get_playurl(client: &Client, ep_id: &str, cid: &str, headers: HeaderMap) -> Result<Value> {
    let url = "https://api.bilibili.com/pgc/player/web/playurl";
    let mut params: HashMap<&str, &str> = HashMap::new();
    params.insert("avid", "");
    params.insert("bvid", "");
    params.insert("ep_id", ep_id);
    params.insert("cid", cid);
    params.insert("qn", "112");
    params.insert("fnval", "16");
    params.insert("fnver", "0");
    params.insert("fourk", "1");
    params.insert("session", "");
    params.insert("from_client", "BROWSER");
    params.insert("drm_tech_type", "2");

    let response = client
        .get(url)
        .headers(headers)
        .query(&params)
        .send()
        .await?;
    let resp_text = response.text().await?;
    let resp_json: Value = serde_json::from_str(&resp_text)?;
    //println!("{:?}", resp_json);

    Ok(resp_json)
}

fn get_file_url(response: Value) -> (String, String) {
    let video_index = {
        let mut max_size_video: i64 = 0;
        let mut index: usize = 0;
        for i in 0..response["result"]["dash"]["video"]
            .as_array()
            .unwrap()
            .len()
        {
            let size = response["result"]["dash"]["video"][i]["size"]
                .as_i64()
                .unwrap_or(0);
            if size > max_size_video {
                max_size_video = size;
                index = i;
            }
        }
        index
    };
    let audio_index = {
        let mut max_size_audio: i64 = 0;
        let mut index: usize = 0;
        for i in 0..response["result"]["dash"]["audio"]
            .as_array()
            .unwrap()
            .len()
        {
            let size = response["result"]["dash"]["audio"][i]["size"]
                .as_i64()
                .unwrap_or(0);
            if size > max_size_audio {
                max_size_audio = size;
                index = i;
            }
        }
        index
    };
    let url_video = response["result"]["dash"]["video"][video_index]["backupUrl"][0]
        .as_str()
        .unwrap_or("");
    let url_audio = response["result"]["dash"]["audio"][audio_index]["backupUrl"][0]
        .as_str()
        .unwrap_or("");
    (url_video.to_string(), url_audio.to_string())
}

async fn get_file(
    url_response: Value,
    name_response: Value,
    ep_id: &str,
    client: &Client,
    headers: HeaderMap,
) -> Result<String> {
    let (url_video, url_audio) = get_file_url(url_response);
    let bangumi_name_temp = get_bangumi_name_from_json(name_response, ep_id);
    let bangumi_name = remove_punctuation(&bangumi_name_temp);

    let video = client
        .get(url_video)
        .headers(headers.clone())
        .send()
        .await?;
    let audio = client
        .get(url_audio)
        .headers(headers.clone())
        .send()
        .await?;

    let mut file_video = File::create(format!("{}_video.mp4", bangumi_name)).unwrap();
    let mut file_audio = File::create(format!("{}_audio.mp3", bangumi_name)).unwrap();
    let video_bytes = video.bytes().await?;
    file_video.write_all(&video_bytes)?;

    let audio_bytes = audio.bytes().await?;
    file_audio.write_all(&audio_bytes)?;
    Ok(bangumi_name)
}

async fn concat_video_audio(name: String) {
    let name_mp4 = format!("{}.mp4", name);
    let name_video = format!("{}_video.mp4", name);
    let name_audio = format!("{}_audio.mp3", name);
    let handle = tokio::spawn(async move {
        let name_mp4 = name_mp4;
        let status = Command::new("ffmpeg")
            .args(&[
                "-i",
                name_video.as_str(),
                "-i",
                name_audio.as_str(),
                "-c:v",
                "copy",
                "-c:v",
                "h264_nvenc",
                "-threads",
                "8",
                "-c:a",
                "aac",
                name_mp4.as_str(),
            ])
            .status()
            .await
            .expect("Failed to execute ffmpeg");

        if status.success() {
            println!("{}", name_mp4);
            std::fs::remove_file(name_video).unwrap();
            std::fs::remove_file(name_audio).unwrap();
        } else {
            eprintln!("Fail!");
        }
    });

    // 等待任务完成
    match handle.await {
        _ => println!("complete"),
        Err(e) => eprintln!("ERROR: {:?}", e),
    }
}

async fn get_bangumi_name(
    client: &Client,
    ep_id: &str,
    season_id: &str,
    headers: HeaderMap,
) -> Result<Value> {
    let url = "https://api.bilibili.com/pgc/view/web/season";
    let mut params: HashMap<&str, &str> = HashMap::new();
    params.insert("ep_id", ep_id);
    params.insert("season_id", season_id);
    let response = client
        .get(url)
        .headers(headers)
        .query(&params)
        .send()
        .await?;
    let resp_text = response.text().await?;
    let resp_text_str = std::str::from_utf8(resp_text.as_bytes()).unwrap();
    let resp_json: Value = serde_json::from_str(resp_text_str)?;

    // let path = Path::new("bangumi_name.json");
    // let mut file = File::create(path).unwrap();
    // file.write_all(serde_json::to_string_pretty(&resp_json).unwrap().as_bytes())
    //     .unwrap();

    Ok(resp_json)
}

fn get_bangumi_name_from_json(json: Value, ep_id: &str) -> String {
    let ep_id = ep_id.parse::<i64>().unwrap();
    let ep_id_index: usize = {
        let mut index: usize = 0;
        for i in 0..json["result"]["episodes"].as_array().unwrap().len() {
            let ep_id_str = json["result"]["episodes"][i]["ep_id"].as_i64().unwrap_or(0);
            if ep_id_str == ep_id {
                index = i;
                break;
            }
        }
        index
    };
    let bangumi_name = json["result"]["episodes"][ep_id_index]["share_copy"]
        .as_str()
        .unwrap();
    bangumi_name.to_string()
}

fn remove_punctuation(input: &str) -> String {
    input
        .chars()
        .filter(|c| !c.is_ascii_punctuation())
        .collect()
}

#[tokio::test]
async fn test_get_playurl() {
    download_bangumi("249943", "").await.unwrap();
}

pub async fn download_bangumi(ep_id: &str, season_id: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let path = Path::new("cookie.txt");
    let cookie = read_cookie(&path);
    let headers = create_headers(&cookie);
    let name_response = get_bangumi_name(&client, &ep_id, season_id, headers.clone()).await?;
    if season_id != "" {
        for i in 0..name_response["result"]["episodes"]
            .as_array()
            .unwrap()
            .len()
        {
            let ep_id_cp = name_response["result"]["episodes"][i]["ep_id"]
                .as_i64()
                .unwrap_or(0)
                .to_string();
            let url_response = get_playurl(&client, &ep_id_cp, "", headers.clone()).await?;
            get_file(
                url_response,
                name_response.clone(),
                &ep_id_cp,
                &client,
                headers.clone(),
            )
            .await?;
        }
    } else {
        let url_response = get_playurl(&client, &ep_id, "", headers.clone()).await?;
        let name = get_file(url_response, name_response, ep_id, &client, headers).await?;
        concat_video_audio(name).await;
    }
    Ok(())
}

use ffmpeg_next::{codec, format, media, util};

async fn concat_video_audio_test() {
    fn main() -> Result<(), Box<dyn std::error::Error>> {
        // 初始化 FFmpeg
        ffmpeg_next::init().unwrap();

        // 输入字节流（模拟 H.264 数据流）
        let input_bytes: Vec<u8> = vec![/* 字节流数据 */];
        let mut input_cursor = std::io::Cursor::new(input_bytes);

        // 输出文件
        let output_path = "output.mp4";

        // 打开输入流
        let mut input_format_context = format::input::from_seekable(&mut input_cursor)?;

        // 创建输出上下文
        let mut output_format_context = format::output(&output_path)?;

        // 复制输入流到输出流
        for stream in input_format_context.streams() {
            if let media::Type::Video = stream.codec().medium() {
                let mut output_stream =
                    output_format_context.add_stream(codec::encoder::find(stream.codec().id())?)?;
                output_stream.set_parameters(stream.parameters())?;
            }
        }

        // 写入文件头
        output_format_context.write_header()?;

        // 解码并写入每帧数据
        for (stream, mut packet) in input_format_context.packets() {
            if let media::Type::Video = input_format_context.stream(stream).codec().medium() {
                packet.rescale_ts(
                    input_format_context.stream(stream).time_base(),
                    output_format_context.stream(stream).time_base(),
                );
                packet.write_interleaved(&mut output_format_context)?;
            }
        }

        // 写入文件尾
        output_format_context.write_trailer()?;
        Ok(())
    }
}
