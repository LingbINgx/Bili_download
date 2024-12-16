use anyhow::{Context, Result};
use reqwest::header::HeaderMap;
use reqwest::Client;
use serde_json::{self, Value};
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use tokio::process::Command;

use crate::refresh_cookie::{create_headers, read_cookie};

pub async fn down_main(url: &str) -> Result<()> {
    let (ep_id, season_id) =
        get_epid_season(url).context("Failed to parse episode ID and season ID from the URL")?;
    download_bangumi(&ep_id, &season_id).await?;
    Ok(())
}

fn get_epid_season(url: &str) -> Result<(String, String)> {
    let parts: Vec<&str> = url.split('?').collect();
    let path_parts: Vec<&str> = parts
        .get(0)
        .context("URL does not contain a valid path")?
        .split('/')
        .collect();

    let id = path_parts
        .last()
        .context("Failed to extract the last part of the URL path")?;
    if id.contains("ep") {
        let ep_id = id.trim_start_matches("ep").to_string();
        Ok((ep_id, String::new()))
    } else if id.contains("ss") {
        let season_id = id.trim_start_matches("ss").to_string();
        Ok((String::new(), season_id))
    } else {
        Err(anyhow::anyhow!(
            "URL does not contain valid episode or season ID"
        ))
    }
}

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

fn get_file_url(response: &Value) -> Result<(String, String)> {
    let video_index = response["result"]["dash"]["video"]
        .as_array()
        .context("Missing or invalid video array in response JSON")?
        .iter()
        .enumerate()
        .max_by_key(|(_, v)| v["size"].as_i64().unwrap_or(0))
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

async fn get_file(
    url_response: Value,
    name_response: Value,
    ep_id: &str,
    client: &Client,
    headers: HeaderMap,
) -> Result<String> {
    let (url_video, url_audio) = get_file_url(&url_response)?;
    let bangumi_name_temp = get_bangumi_name_from_json(name_response, ep_id);
    let bangumi_name = remove_punctuation(&bangumi_name_temp);

    let video_path = format!("{}_video.mp4", bangumi_name);
    let audio_path = format!("{}_audio.mp3", bangumi_name);
    let output_path = format!("{}.mp4", bangumi_name);

    if Path::new(&output_path).exists() {
        println!("{} already exists", bangumi_name);
        return Ok(bangumi_name);
    }

    println!("Downloading {}", bangumi_name);
    let video_bytes = client
        .get(&url_video)
        .headers(headers.clone())
        .send()
        .await
        .context("Failed to download video stream")?
        .bytes()
        .await
        .context("Failed to read video stream data")?;

    let audio_bytes = client
        .get(&url_audio)
        .headers(headers.clone())
        .send()
        .await
        .context("Failed to download audio stream")?
        .bytes()
        .await
        .context("Failed to read audio stream data")?;

    File::create(&video_path)
        .and_then(|mut f| f.write_all(&video_bytes))
        .context("Failed to save video file")?;

    File::create(&audio_path)
        .and_then(|mut f| f.write_all(&audio_bytes))
        .context("Failed to save audio file")?;

    concat_video_audio(bangumi_name.clone()).await?;
    println!("Concat completed for {}", bangumi_name);

    Ok(bangumi_name)
}

async fn concat_video_audio(name: String) -> Result<()> {
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
    handle.await?;
    Ok(())
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
