use crate::down_bangumi;
use crate::down_bv;
use anyhow::{Context, Result};

#[derive(Debug)]
pub struct Video {
    ep_id: String,
    season_id: String,
    bv_id: String,
}

/// 获取网址中的epid/seasonid/bv
pub fn get_epid_season(url: &str) -> Result<Video> {
    let url = url.trim();
    let parts: Vec<&str> = url.split('?').collect();
    let path_parts: Vec<&str> = parts
        .get(0)
        .context("URL does not contain a valid path")?
        .split('/')
        .collect();
    let id = path_parts
        .iter()
        .rev()
        .find(|&&x| !x.is_empty())
        .context("Failed to extract the last part of the URL path")?;
    if id.starts_with("ep") {
        let ep_id = id.trim_start_matches("ep").to_string();
        Ok(Video {
            ep_id,
            season_id: String::new(),
            bv_id: String::new(),
        })
    } else if id.starts_with("ss") {
        let season_id = id.trim_start_matches("ss").to_string();
        Ok(Video {
            ep_id: String::new(),
            season_id,
            bv_id: String::new(),
        })
    } else if id.starts_with("BV") {
        let bv_id = id.to_string();
        Ok(Video {
            ep_id: String::new(),
            season_id: String::new(),
            bv_id,
        })
    } else {
        Err(anyhow::anyhow!(
            "URL does not contain valid episode, season ID or BV ID"
        ))
    }
}

pub async fn choose_download_method(video: &Video) -> Result<String> {
    let mut title = String::new();
    if !video.ep_id.is_empty() || !video.season_id.is_empty() {
        down_bangumi::down_main((&video.ep_id, &video.season_id)).await?;
    } else if !video.bv_id.is_empty() {
        title = down_bv::down_main(&video.bv_id).await?;
    } else {
        Err(anyhow::anyhow!("No valid video ID found"))?;
    }
    Ok(title)
}

pub async fn get_title_pic(video: &Video) -> Result<(String, String)> {
    let mut title = String::new();
    let mut pic = String::new();
    if !video.ep_id.is_empty() || !video.season_id.is_empty() {
        (title, pic) = down_bangumi::bangumi_title(&video.ep_id, &video.season_id).await?;
    } else if !video.bv_id.is_empty() {
        (title, pic) = down_bv::bv_title(&video.bv_id).await?;
    } else {
        Err(anyhow::anyhow!("No valid video ID found"))?;
    }
    Ok((title, pic))
}
