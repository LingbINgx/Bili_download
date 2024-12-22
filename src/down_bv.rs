use crate::down_bangumi::{concat_video_audio, read_cookie_or_not, remove_punctuation};
use crate::refresh_cookie::create_headers;
use crate::wbi::get_wbi_keys_main;
use anyhow::{Context, Ok, Result};
use chrono::Utc;
use futures_util::TryStreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::header::HeaderMap;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{self, Value};
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use tokio::io::AsyncWriteExt;

#[derive(Deserialize, Debug)]
struct BV {
    bv_id: String,
    cid: String,
    title: String,
}

async fn get_bv_play_url(
    client: &Client,
    bv_id: &str,
    cid: &str,
    headers: HeaderMap,
) -> Result<Value> {
    let url = "https://api.bilibili.com/x/player/wbi/playurl";
    let wbi_keys = get_wbi_keys_main().await?;
    let params: HashMap<&str, &str> = [
        ("bvid", bv_id),
        ("cid", cid),
        ("qn", "112"),
        ("fnval", "16"),
        ("fnver", "0"),
        ("fourk", "1"),
        ("session", ""),
        ("from_client", "BROWSER"),
        ("wts", &wbi_keys.wts),
        ("w_rid", &wbi_keys.w_rid),
    ]
    .iter()
    .cloned()
    .collect();

    let resp = client
        .get(url)
        .headers(headers)
        .query(&params)
        .send()
        .await?
        .text()
        .await?;
    let json: Value = serde_json::from_str(&resp)?;
    Ok(json)
}

async fn get_bv_cid_title(client: &Client, bv: &str, headers: HeaderMap) -> Result<BV> {
    let url = "https://api.bilibili.com/x/web-interface/wbi/view";
    let params: HashMap<&str, &str> = [("bvid", bv)].iter().cloned().collect();
    let resp = client
        .get(url)
        .headers(headers)
        .query(&params)
        .send()
        .await?
        .text()
        .await?;
    let json: Value = serde_json::from_str(&resp)?;
    let cid = json["data"]["cid"]
        .as_i64()
        .map(|cid| cid.to_string())
        .unwrap_or_else(|| "".to_string());
    let title = json["data"]["title"]
        .as_str()
        .unwrap_or("no title")
        .to_string();
    let title = remove_punctuation(&title);
    let bv = BV {
        bv_id: bv.to_string(),
        cid: cid,
        title: title,
    };
    Ok(bv)
}

fn get_bv_url(play_url: &Value) -> Result<(String, String)> {
    let video_index = play_url["data"]["dash"]["video"]
        .as_array()
        .context("Missing or invalid video array in response JSON")?
        .iter()
        .enumerate()
        .max_by_key(|(_, v)| v["bandwidth"].as_i64().unwrap_or(0))
        .map(|(i, _)| i)
        .context("No valid video streams found")?;
    let audio_index = play_url["data"]["dash"]["audio"]
        .as_array()
        .context("Missing or invalid audio array in response JSON")?
        .iter()
        .enumerate()
        .max_by_key(|(_, v)| v["bandwidth"].as_i64().unwrap_or(0))
        .map(|(i, _)| i)
        .context("No valid audio streams found")?;
    let video_url = play_url["data"]["dash"]["video"][video_index]["baseUrl"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let audio_url = play_url["data"]["dash"]["audio"][audio_index]["baseUrl"]
        .as_str()
        .unwrap_or("")
        .to_string();
    Ok((video_url, audio_url))
}

async fn down_file_url(url: &str, client: Client, headers: HeaderMap, path: &str) -> Result<()> {
    let resp = client
        .get(url)
        .headers(headers.clone())
        .send()
        .await
        .context("Failed to download stream")?;
    let total_size = resp.content_length().unwrap_or(0);
    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")?
            .progress_chars("=> "),
    );
    let mut file = File::create(&path)?;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.try_next().await? {
        let chunk = chunk;
        file.write_all(&chunk)?;
        pb.inc(chunk.len() as u64);
    }
    pb.finish_with_message("Downloaded video stream");
    Ok(())
}

async fn down_file_bv_(
    client: &Client,
    url: Value,
    name: String,
    headers: HeaderMap,
) -> Result<()> {
    let (video_url, audio_url) = get_bv_url(&url).unwrap();

    if !Path::new("./download").exists() {
        std::fs::create_dir_all("./download")?;
    }
    let video_path = format!("./download/{}_video.m4s", name);
    let audio_path = format!("./download/{}_audio.m4s", name);
    let output_path = format!("./download/{}.mp4", name);

    if Path::new(&output_path).exists() {
        println!("{} already exists", output_path);
        return Ok(());
    }
    println!("downloading {}", name);

    let urls = vec![(video_url, video_path), (audio_url, audio_path)];
    for (url, path) in urls {
        down_file_url(&url, client.clone(), headers.clone(), &path).await?;
    }

    concat_video_audio(name.clone()).await?;
    println!("Concat completed for {}", name);
    Ok(())
}

// #[tokio::test]
// async fn test_() {
//     let client = reqwest::Client::new();
//     let path = Path::new("load");
//     let cookies = read_cookie_or_not(path).await.unwrap();
//     let headers = create_headers(&cookies);

//     let bv_id = "BV1yaBKYfE2D";
//     let bv = get_bv_cid_title(&client, bv_id, headers.clone())
//         .await
//         .unwrap();
//     print!("{:#?}\n", bv);
//     let x = get_bv_play_url(&client, &bv.bv_id, &bv.cid, headers.clone())
//         .await
//         .unwrap();
//     down_file_bv_(&client, x, bv.title, headers).await.unwrap();
// }

async fn bv_down_main(bv_id: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let path = Path::new("load");
    let cookies = read_cookie_or_not(path).await?;
    let headers = create_headers(&cookies);
    let bv = get_bv_cid_title(&client, bv_id, headers.clone())
        .await
        .context("Failed to get bv cid title")?;
    println!("{:#?}", bv);
    let play_url = get_bv_play_url(&client, &bv.bv_id, &bv.cid, headers.clone())
        .await
        .context("Failed to get bv play url")?;
    let time = Utc::now() + chrono::Duration::hours(8);
    let time_ = time.format("%Y-%m-%d %H:%M:%S");
    let data = format!("{}\t{}\t{}\t\n", time_, bv.bv_id, bv.title);
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
    down_file_bv_(&client, play_url, bv.title, headers).await?;
    Ok(())
}

pub async fn down_main(bv_id: &str) -> Result<()> {
    bv_down_main(bv_id).await?;
    Ok(())
}
