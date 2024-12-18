use crate::down_bangumi::{concat_video_audio, read_cookie_or_not, remove_punctuation};
use crate::refresh_cookie::create_headers;
use crate::wbi::get_wbi_keys_main;
use anyhow::{Context, Ok, Result};
use reqwest::header::HeaderMap;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{self, Value};
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;

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

async fn download_file(
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
        println!("./download/{} already exists", output_path);
        return Ok(());
    }

    println!("is downloading {}", output_path);
    let video_bytes = client
        .get(&video_url)
        .headers(headers.clone())
        .send()
        .await
        .context("Failed to download video stream")?
        .bytes()
        .await
        .context("Failed to read video stream data")?;

    let audio_bytes = client
        .get(&audio_url)
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

    concat_video_audio(name.clone()).await?;
    println!("Concat completed for {}", name);
    Ok(())
}

#[tokio::test]
async fn test_() {
    let client = reqwest::Client::new();
    let path = Path::new("cookie.text");
    let cookies = read_cookie_or_not(path).unwrap();
    let headers = create_headers(&cookies);

    let bv_id = "BV1yaBKYfE2D";
    let bv = get_bv_cid_title(&client, bv_id, headers.clone())
        .await
        .unwrap();
    print!("{:#?}\n", bv);
    let x = get_bv_play_url(&client, &bv.bv_id, &bv.cid, headers.clone())
        .await
        .unwrap();
    download_file(&client, x, bv.title, headers).await.unwrap();
}

async fn bv_down_main(bv_id: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let path = Path::new("cookie.txt");
    let cookies = read_cookie_or_not(path).unwrap();
    let headers = create_headers(&cookies);
    let bv = get_bv_cid_title(&client, bv_id, headers.clone())
        .await
        .context("Failed to get bv cid title")?;
    println!("{:#?}", bv);
    let play_url = get_bv_play_url(&client, &bv.bv_id, &bv.cid, headers.clone())
        .await
        .context("Failed to get bv play url")?;
    download_file(&client, play_url, bv.title, headers).await?;
    Ok(())
}

pub async fn down_main(bv_id: &str) -> Result<()> {
    bv_down_main(bv_id).await?;
    Ok(())
}
