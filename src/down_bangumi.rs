use anyhow::{Context, Ok, Result};
use futures_util::TryStreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::header::HeaderMap;
use reqwest::Client;
use serde_json::{self, Value};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{self, Path};
use tokio::process::Command;

use crate::refresh_cookie::{create_headers, Cookies};

// pub async fn _down_main(url: &str) -> Result<()> {
//     let (ep_id, season_id) =
//         get_epid_season(url).context("Failed to parse episode ID and season ID from the URL")?;
//     println!("ep_id: {}, season_id: {}", ep_id, season_id);
//     download_bangumi(&ep_id, &season_id).await?;
//     Ok(())
// }

pub async fn down_main((ep_id, season_id): (&str, &str)) -> Result<()> {
    download_bangumi(ep_id, season_id).await?;
    Ok(())
}

/// 获取视频播放地址
async fn get_playurl(client: &Client, ep_id: &str, cid: &str, headers: HeaderMap) -> Result<Value> {
    let url = "https://api.bilibili.com/pgc/player/web/playurl";
    let params: HashMap<&str, &str> = [
        ("avid", ""),
        ("bvid", ""),
        ("ep_id", ep_id),
        ("cid", cid),
        ("qn", "112"),
        ("fnval", "16"),
        ("fnver", "0"),
        ("fourk", "1"),
        ("session", ""),
        ("from_client", "BROWSER"),
        ("drm_tech_type", "2"),
    ]
    .iter()
    .cloned()
    .collect();

    let response = client
        .get(url)
        .headers(headers)
        .query(&params)
        .send()
        .await
        .context("Failed to send request to Bilibili play URL API")?;

    let resp_text = response
        .text()
        .await
        .context("Failed to read response text from play URL API")?;
    let resp_json: Value = serde_json::from_str(&resp_text)
        .context("Failed to parse JSON response from play URL API")?;

    Ok(resp_json)
}

/// 获取json文件中的视频文件地址
fn get_file_url(response: &Value) -> Result<(String, String)> {
    let video_index = response["result"]["dash"]["video"]
        .as_array()
        .context("Missing or invalid video array in response JSON")?
        .iter() //迭代器
        .enumerate() //枚举
        .max_by_key(|(_, v)| v["size"].as_i64().unwrap_or(0)) // 索引 键值，根据size排序
        .map(|(i, _)| i)
        .context("No valid video streams found")?;

    let audio_index = response["result"]["dash"]["audio"]
        .as_array()
        .context("Missing or invalid audio array in response JSON")?
        .iter()
        .enumerate()
        .max_by_key(|(_, a)| a["size"].as_i64().unwrap_or(0))
        .map(|(i, _)| i)
        .context("No valid audio streams found")?;

    let url_video = response["result"]["dash"]["video"][video_index]["backupUrl"]
        .get(0)
        .and_then(|u| u.as_str())
        .context("No backup URL found for video stream")?;
    let url_audio = response["result"]["dash"]["audio"][audio_index]["backupUrl"]
        .get(0)
        .and_then(|u| u.as_str())
        .context("No backup URL found for audio stream")?;

    Ok((url_video.to_string(), url_audio.to_string()))
}

/// 下载视频文件
// async fn get_file(
//     url_response: Value,
//     name_response: Value,
//     ep_id: &str,
//     client: &Client,
//     headers: HeaderMap,
// ) -> Result<()> {
//     let (url_video, url_audio) = get_file_url(&url_response)?;
//     let bangumi_name_temp = get_bangumi_name_from_json(name_response, ep_id);
//     let bangumi_name = remove_punctuation(&bangumi_name_temp);
//     if !Path::new("./download").exists() {
//         std::fs::create_dir_all("./download")?;
//     }
//     let video_path = format!("./download/{}_video.m4s", bangumi_name);
//     let audio_path = format!("./download/{}_audio.m4s", bangumi_name);
//     let output_path = format!("./download/{}.mp4", bangumi_name);

//     if Path::new(&output_path).exists() {
//         println!("{} already exists", bangumi_name);
//         return Ok(());
//     }

//     println!("is downloading {}", bangumi_name);
//     let video_bytes = client
//         .get(&url_video)
//         .headers(headers.clone())
//         .send()
//         .await
//         .context("Failed to download video stream")?
//         .bytes()
//         .await
//         .context("Failed to read video stream data")?;

//     let audio_bytes = client
//         .get(&url_audio)
//         .headers(headers.clone())
//         .send()
//         .await
//         .context("Failed to download audio stream")?
//         .bytes()
//         .await
//         .context("Failed to read audio stream data")?;

//     File::create(&video_path)
//         .and_then(|mut f| f.write_all(&video_bytes))
//         .context("Failed to save video file")?;

//     File::create(&audio_path)
//         .and_then(|mut f| f.write_all(&audio_bytes))
//         .context("Failed to save audio file")?;

//     concat_video_audio(bangumi_name.clone()).await?;
//     println!("Concat completed for {}", bangumi_name);

//     Ok(())
// }

async fn down_from_url(url: String, client: &Client, headers: HeaderMap, path: &str) -> Result<()> {
    let resp = client
        .get(&url)
        .headers(headers.clone())
        .send()
        .await
        .context("Failed to download video stream")?;
    let total_size_video = resp.content_length().unwrap_or(0);
    let pb = ProgressBar::new(total_size_video);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")?
            .progress_chars("=> "),
    );
    let mut file = File::create(&path)?;
    let mut downloaded: u64 = 0;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.try_next().await? {
        let chunk = chunk;
        file.write_all(&chunk)?;
        pb.inc(chunk.len() as u64);
        downloaded += chunk.len() as u64;
        pb.set_position(downloaded);
    }
    pb.finish_with_message("Downloaded video stream");
    Ok(())
}

async fn down_file_bangumi(
    url_response: Value,
    name_response: Value,
    ep_id: &str,
    client: &Client,
    headers: HeaderMap,
) -> Result<()> {
    let (url_video, url_audio) = get_file_url(&url_response)?;
    let bangumi_name_temp = get_bangumi_name_from_json(name_response, ep_id);
    let bangumi_name = remove_punctuation(&bangumi_name_temp);
    if !Path::new("./download").exists() {
        std::fs::create_dir_all("./download")?;
    }
    let video_path = format!("./download/{}_video.m4s", bangumi_name);
    let audio_path = format!("./download/{}_audio.m4s", bangumi_name);
    let output_path = format!("./download/{}.mp4", bangumi_name);

    if Path::new(&output_path).exists() {
        println!("{} already exists", bangumi_name);
        return Ok(());
    }
    println!("downloading {}", bangumi_name);

    down_from_url(url_video, client, headers.clone(), &video_path).await?;
    down_from_url(url_audio, client, headers.clone(), &audio_path).await?;

    concat_video_audio(bangumi_name.clone()).await?;
    println!("Concat completed for {}", bangumi_name);
    Ok(())
}

/// 合并视频和音频文件
pub async fn concat_video_audio(name: String) -> Result<()> {
    if !Path::new("./download").exists() {
        std::fs::create_dir_all("./download")?;
    }
    let name_mp4 = format!("./download/{}.mp4", name);
    let name_video = format!("./download/{}_video.m4s", name);
    let name_audio = format!("./download/{}_audio.m4s", name);
    let handle = tokio::spawn(async move {
        let name_mp4 = name_mp4;
        if Path::new(&name_mp4).exists() {
            return;
        }
        let status = Command::new("ffmpeg")
            .args(&[
                "-loglevel",
                "error",
                "-i",
                name_video.as_str(),
                "-i",
                name_audio.as_str(),
                "-c:v",
                "copy",
                // "-c:v",
                // "h264_nvenc",
                // "-threads",
                // "8",
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
    handle.await?;
    Ok(())
}

/// 获取番剧名称
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

    Ok(resp_json)
}

/// 从json文件中获取该ep_id对应的番剧名称
fn get_bangumi_name_from_json(json: Value, ep_id: &str) -> String {
    let ep_id = ep_id.parse::<i64>().unwrap();
    let ep_id_index: usize = json["result"]["episodes"]
        .as_array()
        .unwrap()
        .iter()
        .position(|episode| episode["ep_id"].as_i64().unwrap_or(0) == ep_id)
        .unwrap_or(0);
    let bangumi_name = json["result"]["episodes"][ep_id_index]["share_copy"]
        .as_str()
        .unwrap();
    bangumi_name.to_string()
}

/// 去除文件名字符串中的windows不允许的标点符号
pub fn remove_punctuation(input: &str) -> String {
    let invalid_chars = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
    input
        .chars()
        .filter(|c| !invalid_chars.contains(c))
        .collect()
}

pub fn read_cookie_or_not(path: &Path) -> Result<Cookies> {
    if path.exists() {
        //println!("{:?} exists", path);
        let mut file = File::open(path)?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        let cookie: Cookies = serde_json::from_str(&content)?;
        return Ok(cookie);
    } else {
        println!("{:?} does not exist", path);
    }
    return Ok(Cookies {
        SESSDATA: String::new(),
        bili_jct: String::new(),
        refresh_token: String::new(),
    });
}

async fn down_season(
    ep_id_cp: String,
    client: &Client,
    headers: HeaderMap,
    name_response: Value,
) -> Result<()> {
    let url_response = get_playurl(&client, &ep_id_cp, "", headers.clone()).await?;
    down_file_bangumi(
        url_response,
        name_response.clone(),
        &ep_id_cp,
        &client,
        headers.clone(),
    )
    .await?;
    Ok(())
}

/// 下载番剧总函数
async fn download_bangumi(ep_id: &str, season_id: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let path = Path::new("./cookie.txt");
    let cookie = read_cookie_or_not(&path)?;
    let headers = create_headers(&cookie);
    let name_response = get_bangumi_name(&client, &ep_id, &season_id, headers.clone()).await?;
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
            down_season(ep_id_cp, &client, headers.clone(), name_response.clone()).await?;
        }
    } else {
        let url_response = get_playurl(&client, &ep_id, "", headers.clone()).await?;
        down_file_bangumi(url_response, name_response, ep_id, &client, headers).await?;
    }
    Ok(())
}
