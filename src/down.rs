use anyhow::{Ok, Result};
use reqwest::header::HeaderMap;
use reqwest::Client;
use serde_json::{self, Value};
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use tokio::process::Command;

use crate::refresh_cookie::{create_headers, read_cookie};

pub async fn down_main(url: &str) {
    let (ep_id, season_id) = get_epid_season(url);
    download_bangumi(&ep_id, &season_id).await.unwrap();
}

fn get_epid_season(url: &str) -> (String, String) {
    let mut ep_id = "";
    let mut season_id = "";
    let url = url;
    let parts: Vec<&str> = url.split("?").collect();
    let path_parts: Vec<&str> = parts[0].split("/").collect();
    let id = path_parts[path_parts.len() - 1];
    if id.contains("ep") {
        ep_id = id;
        ep_id = ep_id.trim_start_matches("ep");
    } else if id.contains("ss") {
        season_id = id;
        season_id = season_id.trim_start_matches("ss");
    }
    (ep_id.to_string(), season_id.to_string())
}

#[test]
fn test_down_main() {
    let (x,y) =get_epid_season("https://www.bilibili.com/bangumi/play/ep249944?spm_id_from=333.1387.0.0&from_spmid=666.25.episode.0");
    println!("{:?}", x);
    println!("{:?}", y);
}

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

    if Path::new(&format!("{}.mp4", bangumi_name)).exists() {
        println!("{} already exists", bangumi_name);
        return Ok(bangumi_name);
    } else {
        println!("Downloading {}", bangumi_name);
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

        concat_video_audio(bangumi_name.to_string()).await;
        println!("concat completed {}", bangumi_name);
    }
    Ok(bangumi_name)
}

async fn concat_video_audio(name: String) {
    let name_mp4 = format!("{}.mp4", name);
    let name_video = format!("{}_video.mp4", name);
    let name_audio = format!("{}_audio.mp3", name);
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
                // "-c:v",
                // "h264_nvenc",
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
    handle.await.unwrap();
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

async fn download_bangumi(ep_id: &str, season_id: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let path = Path::new("cookie.txt");
    let cookie = read_cookie(&path);
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
        get_file(url_response, name_response, ep_id, &client, headers).await?;
    }
    Ok(())
}
