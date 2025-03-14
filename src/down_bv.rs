use crate::down_bangumi::{concat_video_audio, read_cookie_or_not, remove_punctuation};
use crate::refresh_cookie::create_headers;
use crate::resolution;
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
    rsl: &str,
) -> Result<Value> {
    let url = "https://api.bilibili.com/x/player/wbi/playurl";
    let wbi_keys = get_wbi_keys_main().await?;
    let qn = resolution::qn(rsl);
    let fnval = resolution::fnval(rsl);
    println!("fnval: {}", fnval);
    println!("qn: {}", qn);
    let params: HashMap<&str, &str> = [
        ("bvid", bv_id),
        ("cid", cid),
        ("qn", qn),
        ("fnval", fnval),
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

fn get_bv_url(play_url: &Value, rsl: &str) -> Result<(String, String, i32)> {
    let qn: i32 = resolution::qn(rsl).parse().unwrap();
    let video_index = play_url["data"]["dash"]["video"]
        .as_array()
        .context("Missing or invalid video array in response JSON")?
        .iter()
        .enumerate()
        .filter(|(_, v)| v["id"] == qn)
        .max_by_key(|(_, v)| v["bandwidth"].as_u64().unwrap_or(0))
        .map(|(index, _)| index)
        .unwrap_or(1);
    println!("video_index: {}", video_index);

    let audio_index = play_url["data"]["dash"]["audio"]
        .as_array()
        .context("Missing or invalid audio array in response JSON")?
        .iter()
        .enumerate()
        .max_by_key(|(_, v)| v["bandwidth"].as_i64().unwrap_or(0))
        .map(|(i, _)| i)
        .context("No valid audio streams found")?;
    println!("audio_index: {}", audio_index);

    let video_url = play_url["data"]["dash"]["video"][video_index]["baseUrl"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let audio_url = play_url["data"]["dash"]["audio"][audio_index]["baseUrl"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let qn = play_url["data"]["dash"]["video"][video_index]["id"]
        .as_i64()
        .unwrap_or(0) as i32;
    Ok((video_url, audio_url, qn))
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
    rsl: &str,
    bv_id: &str,
) -> Result<()> {
    let (video_url, audio_url, qn) =
        get_bv_url(&url, rsl).unwrap_or((String::new(), String::new(), 0));

    let qn_c = resolution::qn(rsl);
    if qn != qn_c.parse::<i32>().unwrap() {
        println!("此分辨率不存在，将下载默认分辨率");
    }
    let qn_str = qn.to_string();
    let rsl = resolution::rsl(&qn_str);

    if !Path::new("./download").exists() {
        std::fs::create_dir_all("./download")?;
    }

    let name = format!("{} {}", name, rsl);
    let video_path = format!("./download/{}_video.m4s", name);
    let audio_path = format!("./download/{}_audio.m4s", name);
    let output_path = format!("./download/{}.mp4", name);

    let time = Utc::now() + chrono::Duration::hours(8);
    let time_ = time.format("%Y-%m-%d %H:%M:%S");
    let data = format!("{}\t{}\t{}\t\n", time_, bv_id, name);
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

async fn bv_down_main(bv_id: &str, rsl: &str) -> Result<String> {
    let client = reqwest::Client::new();
    let path = Path::new("load");
    let cookies = read_cookie_or_not(path).await?;
    let headers = create_headers(&cookies);
    let bv = get_bv_cid_title(&client, bv_id, headers.clone())
        .await
        .context("Failed to get bv cid title")?;
    println!("{:#?}", bv);

    let play_url = get_bv_play_url(&client, &bv.bv_id, &bv.cid, headers.clone(), rsl)
        .await
        .context("Failed to get bv play url")?;
    down_file_bv_(&client, play_url, bv.title.clone(), headers, rsl, &bv.bv_id).await?;
    Ok(bv.title)
}

pub async fn down_main(bv_id: &str, rsl: &str) -> Result<String> {
    let title = bv_down_main(bv_id, rsl).await?;
    Ok(title)
}

pub async fn bv_title(bv_id: &str) -> Result<(String, String)> {
    let client = reqwest::Client::new();
    let path = Path::new("load");
    let cookies = read_cookie_or_not(path).await?;
    let headers = create_headers(&cookies);
    let url = "https://api.bilibili.com/x/web-interface/wbi/view";
    let params: HashMap<&str, &str> = [("bvid", bv_id)].iter().cloned().collect();
    let resp = client
        .get(url)
        .headers(headers)
        .query(&params)
        .send()
        .await?
        .text()
        .await?;
    let json: Value = serde_json::from_str(&resp)?;
    let title = json["data"]["title"]
        .as_str()
        .unwrap_or("no title")
        .to_string();
    let pic = json["data"]["pic"].as_str().unwrap_or("no pic").to_string();
    let title = remove_punctuation(&title);
    get_pic(&pic).await?;
    Ok((title, pic))
}

pub async fn get_pic(pic: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let path = Path::new("load");
    let cookies = read_cookie_or_not(path).await?;
    let headers = create_headers(&cookies);
    let resp = client.get(pic).headers(headers).send().await?;
    let bytes = resp.bytes().await?;
    let path = Path::new("pic.png");
    let mut file = tokio::fs::File::create(path).await?;
    file.write_all(&bytes).await?;
    Ok(())
}
