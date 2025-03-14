use anyhow::{Context, Ok, Result};
use chrono::Utc;
use futures_util::TryStreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::header::HeaderMap;
use reqwest::Client;
use serde_json::{self, Value};
use std::collections::HashMap;
use std::path::Path;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

use crate::down_bv::get_pic;
use crate::refresh_cookie::{create_headers, Cookies};
use crate::resolution;

pub async fn down_main((ep_id, season_id): (&str, &str), rsl: &str) -> Result<()> {
    download_bangumi(ep_id, season_id, rsl).await?;
    Ok(())
}

/// 获取视频播放地址
async fn get_playurl(
    client: &Client,
    ep_id: &str,
    cid: &str,
    headers: HeaderMap,
    rsl: &str,
) -> Result<Value> {
    let url = "https://api.bilibili.com/pgc/player/web/playurl";
    let qn = resolution::qn(rsl);
    let fnval = resolution::fnval(rsl);
    println!("fnval: {}", fnval);
    println!("qn: {}", qn);
    let params: HashMap<&str, &str> = [
        ("avid", ""),
        ("bvid", ""),
        ("ep_id", ep_id),
        ("cid", cid),
        ("qn", qn),
        ("fnval", fnval),
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
fn get_file_url(response: &Value, rsl: &str) -> Result<(String, String, i32)> {
    let qn: i32 = resolution::qn(rsl).parse().unwrap();
    println!("get file url qn: {}", qn);
    let video_index = response["result"]["dash"]["video"]
        .as_array()
        .context("Missing or invalid video array in response JSON")?
        .iter()
        .enumerate()
        .filter(|(_, v)| v["id"] == qn)
        .max_by_key(|(_, v)| v["bandwidth"].as_u64().unwrap_or(0))
        .map(|(index, _)| index)
        .unwrap_or(0);
    println!("video_index: {}", video_index);

    let audio_index = response["result"]["dash"]["audio"]
        .as_array()
        .context("Missing or invalid audio array in response JSON")?
        .iter()
        .enumerate()
        .max_by_key(|(_, a)| a["size"].as_i64().unwrap_or(0))
        .map(|(i, _)| i)
        .context("No valid audio streams found")?;

    let url_video = response["result"]["dash"]["video"][video_index]["baseUrl"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let url_audio = response["result"]["dash"]["audio"][audio_index]["baseUrl"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let qn = response["result"]["dash"]["video"][video_index]["id"]
        .as_i64()
        .unwrap_or(0) as i32;

    Ok((url_video.to_string(), url_audio.to_string(), qn))
}

async fn down_from_url(url: &str, client: Client, headers: HeaderMap, path: &str) -> Result<()> {
    let resp = client
        .get(url)
        .headers(headers.clone())
        .send()
        .await
        .context("Failed to download video stream")?;
    let total_size = resp.content_length().unwrap_or(0);
    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta}) ")?
            .progress_chars("=> "),
    );

    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    let mut file = File::create(&path).await?;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.try_next().await? {
        let chunk = chunk;
        file.write_all(&chunk).await?;

        pb.inc(chunk.len() as u64);
    }
    pb.set_position(total_size);
    pb.finish_with_message("Downloaded stream");
    Ok(())
}

/// 下载番剧文件
async fn down_file_bangumi(
    url_response: Value,
    name_response: Value,
    ep_id: &str,
    client: &Client,
    headers: HeaderMap,
    rsl: &str,
) -> Result<()> {
    let (url_video, url_audio, qn) = get_file_url(&url_response, rsl)?;
    let qn_c = resolution::qn(rsl);
    if qn != qn_c.parse::<i32>().unwrap() {
        println!("此分辨率不存在，将下载默认分辨率");
    }
    let qn_str = qn.to_string();
    let rsl = resolution::rsl(&qn_str);

    let bangumi_name_temp = get_bangumi_name_from_json(name_response, ep_id);
    let bangumi_name = remove_punctuation(&bangumi_name_temp);

    let bangumi_name = format!("{} {}", bangumi_name, rsl);

    let time = Utc::now() + chrono::Duration::hours(8);
    let time_ = time.format("%Y-%m-%d %H:%M:%S");
    let data = format!("{}\tep{}\t{}\t\n", time_, ep_id, bangumi_name);
    let path = Path::new("dat.log");
    if !path.exists() {
        let mut file = tokio::fs::File::create(path).await?;
        file.write_all(data.as_bytes()).await?;
    } else {
        let mut file = tokio::fs::OpenOptions::new()
            .append(true)
            .open(path)
            .await?;
        file.write_all(data.as_bytes()).await?;
    }

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

    let urls = vec![(url_video, video_path), (url_audio, audio_path)];
    for (url, path) in urls {
        let client = client.clone();
        let headers = headers.clone();
        down_from_url(&url, client, headers, &path).await?;
    }

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
                "-i",
                name_video.as_str(),
                "-i",
                name_audio.as_str(),
                "-c:v",
                "copy",
                "-c:a",
                "copy",
                "-shortest",
                "-map",
                "0:v",
                "-map",
                "1:a",
                "-y",
                "-movflags",
                "+faststart",
                name_mp4.as_str(),
                "-hide_banner",
                "-stats",
                "-loglevel",
                "error",
            ])
            .stdin(std::process::Stdio::null())
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
    let resp_text_str = std::str::from_utf8(resp_text.as_bytes()).unwrap_or("");
    let resp_json: Value = serde_json::from_str(resp_text_str)?;

    Ok(resp_json)
}

/// 从json文件中获取该ep_id对应的番剧名称
fn get_bangumi_name_from_json(json: Value, ep_id: &str) -> String {
    let ep_id = ep_id.parse::<i64>().unwrap();
    let ep_id_index: usize = json["result"]["episodes"]
        .as_array()
        .unwrap_or(&std::vec::Vec::new())
        .iter()
        .position(|episode| episode["ep_id"].as_i64().unwrap_or(0) == ep_id)
        .unwrap_or(0);
    let bangumi_name = json["result"]["episodes"][ep_id_index]["share_copy"]
        .as_str()
        .unwrap_or("");
    bangumi_name.to_string()
}

///
fn get_bangumi_pic(json: Value, ep_id: &str) -> String {
    let ep_id = ep_id.parse::<i64>().unwrap();
    let ep_id_index: usize = json["result"]["episodes"]
        .as_array()
        .unwrap_or(&std::vec::Vec::new())
        .iter()
        .position(|episode| episode["ep_id"].as_i64().unwrap_or(0) == ep_id)
        .unwrap_or(0);
    let bangumi_pic = json["result"]["episodes"][ep_id_index]["cover"]
        .as_str()
        .unwrap_or("");
    bangumi_pic.to_string()
}

/// 去除文件名字符串中的windows不允许的标点符号
pub fn remove_punctuation(input: &str) -> String {
    let invalid_chars = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
    input
        .chars()
        .filter(|c| !invalid_chars.contains(c))
        .collect()
}

pub async fn read_cookie_or_not(path: &Path) -> Result<Cookies> {
    if path.exists() {
        //println!("{:?} exists", path);
        let mut file = File::open(path).await?;
        let mut content = String::new();
        file.read_to_string(&mut content).await?;
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
    rsl: &str,
) -> Result<()> {
    let url_response = get_playurl(&client, &ep_id_cp, "", headers.clone(), rsl).await?;
    down_file_bangumi(
        url_response,
        name_response.clone(),
        &ep_id_cp,
        &client,
        headers.clone(),
        rsl,
    )
    .await?;
    Ok(())
}

/// 下载番剧总函数
async fn download_bangumi(ep_id: &str, season_id: &str, rsl: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let path = Path::new("./load");
    let cookie = read_cookie_or_not(&path).await?;
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
            down_season(
                ep_id_cp,
                &client,
                headers.clone(),
                name_response.clone(),
                rsl,
            )
            .await?;
        }
    } else {
        let url_response = get_playurl(&client, &ep_id, "", headers.clone(), rsl).await?;
        //println!("{:#}", url_response);
        down_file_bangumi(url_response, name_response, ep_id, &client, headers, rsl).await?;
    }
    Ok(())
}

pub async fn bangumi_title(ep_id: &str, season_id: &str) -> Result<(String, String)> {
    let client = reqwest::Client::new();
    let path = Path::new("./load");
    let cookie = read_cookie_or_not(&path).await?;
    let headers = create_headers(&cookie);
    let name_response = get_bangumi_name(&client, &ep_id, &season_id, headers.clone()).await?;
    let mut bangumi_name_temp = String::new();
    let mut bangumi_pic = String::new();
    if ep_id != "" {
        bangumi_name_temp = get_bangumi_name_from_json(name_response.clone(), ep_id);
        bangumi_pic = get_bangumi_pic(name_response, ep_id);
    } else {
        bangumi_pic = name_response["result"]["cover"]
            .as_str()
            .unwrap_or("")
            .to_string();
        bangumi_name_temp = name_response["result"]["title"]
            .as_str()
            .unwrap_or("")
            .to_string();
    }

    let bangumi_name = remove_punctuation(&bangumi_name_temp);
    get_pic(&bangumi_pic).await?;
    Ok((bangumi_name, bangumi_pic))
}
