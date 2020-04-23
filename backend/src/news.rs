use anyhow::{Error, Result};
use chrono::prelude::*;
use directories::ProjectDirs;
use futures::prelude::*;
use futures::future::{join_all, ok, err};
use rayon::prelude::*;
use rss::Channel;
use serde::{Deserialize, Serialize};

use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};
use std::fmt;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

pub use rss;

pub async fn read_news() -> Result<Vec<NewsItem>> {
    let proj_dirs = ProjectDirs::from("com", "Big Endian", "News App")
        .ok_or(Error::msg("Failure to get project directory."))?;

    let feeds = [
        "http://feeds.arstechnica.com/arstechnica/index",
        "https://boingboing.net/feed",
        "http://rss.slashdot.org/Slashdot/slashdotMain",
        "https://hackaday.com/blog/feed/",
        "https://www.phoronix.com/rss.php",
        //"https://www.theatlantic.com/feed/all/",
        "https://www.newyorker.com/feed/everything",
    ];
    let channels: Vec<rss::Channel> = feeds
        .iter()
        .map(|url| Channel::from_url(url).unwrap())
        .collect();
    log::trace!("loaded channels.");
    let items: Vec<&rss::Item> = channels.iter().map(|ch| ch.items()).flatten().collect();

    let cache_dir = proj_dirs.cache_dir();
    fs::create_dir_all(cache_dir)?;

    log::trace!("done gathering items");
    let news_items: Vec<NewsItem> = items
        .into_iter()
        .map(|x| NewsItem::new(x.clone(), &cache_dir))
        .collect();

    let image_urls: Vec<String> = news_items.iter().filter_map(|item| item.image_url()).collect();
    let dl_futures = image_urls.iter().map(|image_url| {
        // Create path we'll use to store associated image.
        let path = image_url.replace("https://", "");
        let path = path.replace("http://", "");
        let path = cache_dir.join(path);

        reqwest::get(image_url)
            .and_then(|resp| resp.bytes())
            .and_then(move |bytes| {
                if !path.exists() {
                    let img = image::load_from_memory(&bytes).unwrap();
                    fs::create_dir_all(path.parent().unwrap()).unwrap();
                    img.save(&path).unwrap();
                }

                ok(())
            })
    });
    join_all(dl_futures).await;

    // TODO rework this.
    let mut existing_items: Vec<NewsItem> =
        if let Ok(file) = fs::File::open(proj_dirs.cache_dir().join("news_items.dat")) {
            log::trace!("opened news_items.dat file");
            bincode::deserialize_from(file)?
        } else {
            Vec::new()
        };

    existing_items.extend(news_items);
    // Take all of the existing items and store them in a set to de-duplicate them.
    let mut items_set = BTreeSet::new();
    existing_items.into_iter().for_each(|item| {
        items_set.insert(item.clone());
    });
    let mut existing_items: Vec<&NewsItem> = items_set.iter().collect();
    let file = fs::File::create(proj_dirs.cache_dir().join("news_items.dat"))?;
    bincode::serialize_into(file, &existing_items)?;

    existing_items.reverse();
    log::trace!("combined all the items together");

    Ok(items_set.into_iter().collect())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsItem {
    item: rss::Item,
    pub image_path: Option<PathBuf>,
    pub_date: Option<chrono::DateTime<chrono::FixedOffset>>,
}

impl fmt::Display for NewsItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} - {}",
            self.title().unwrap_or(""),
            self.description().unwrap_or("")
        )
    }
}

impl PartialEq for NewsItem {
    fn eq(&self, other: &Self) -> bool {
        self.item.title() == other.item.title()
            && self.item.description() == other.item.description()
            && self.item.pub_date() == other.item.pub_date()
    }
}

trait NeededData {
    fn image_url(&self) -> Option<String>;
    fn publish_date(&self) -> Option<chrono::DateTime<chrono::FixedOffset>>;
    fn digest(&self) -> blake3::Hash;
}

impl NeededData for rss::Item {
    fn digest(&self) -> blake3::Hash {
        let mut hasher = blake3::Hasher::new();
        hasher.update(self.title().unwrap().as_bytes());
        hasher.update(self.description().unwrap().as_bytes());
        hasher.finalize()
    }

    fn publish_date(&self) -> Option<DateTime<FixedOffset>> {
        let pub_date = self.pub_date();
        if let Some(date) = pub_date {
            return DateTime::parse_from_str(date, "%a, %d %b %Y %H:%M:%S %z").ok();
        }

        if let Some(dc) = self.dublin_core_ext() {
            let dates = dc.dates();
            if !dates.is_empty() {
                //println!("Dates: {:#?}", dates[0].parse::<DateTime<Utc>>());
                return dates[0].parse::<DateTime<FixedOffset>>().ok();
            }
        }

        None
    }

    fn image_url(&self) -> Option<String> {
        if let Some(media) = self.extensions().get("media") {
            if let Some(thumbnail) = media.get("thumbnail") {
                if let Some(thumbnail) = thumbnail.first() {
                    if let Some(url) = thumbnail.attrs().get("url") {
                        return Some(url.to_owned());
                    }
                }
            }
        }
        None
    }
}

impl NewsItem {
    pub fn new(item: rss::Item, cache_dir: &Path) -> Self {
        let pub_date = item.publish_date();

        let image_path = item.image_url().map(|image_url| {
            let image_path = image_url.replace("https://", "");
            let image_path = image_path.replace("http://", "");
            let image_path = cache_dir.join(image_path);
            image_path
        });

        NewsItem {
            item,
            pub_date,
            image_path,
        }
    }

    pub fn pub_date(&self) -> Option<chrono::DateTime<chrono::FixedOffset>> {
        self.pub_date
    }

    pub fn digest(&self) -> blake3::Hash {
        self.item.digest()
    }

    pub fn title(&self) -> Option<&str> {
        self.item.title()
    }

    pub fn description(&self) -> Option<&str> {
        self.item.description()
    }

    pub fn image_url(&self) -> Option<String> {
        self.item.image_url()
    }
}

impl Hash for NewsItem {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.item.title().hash(state);
        self.item.description().hash(state);
        self.item.pub_date().hash(state);
    }
}

impl Eq for NewsItem {}

impl Ord for NewsItem {
    fn cmp(&self, other: &Self) -> Ordering {
        self.pub_date().cmp(&other.pub_date())
    }
}

impl PartialOrd for NewsItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Return the publish date for the item.
pub fn publish_date(item: &rss::Item) -> Option<DateTime<FixedOffset>> {
    let pub_date = item.pub_date();
    if let Some(date) = pub_date {
        return DateTime::parse_from_str(date, "%a, %d %b %Y %H:%M:%S %z").ok();
    }

    if let Some(dc) = item.dublin_core_ext() {
        let dates = dc.dates();
        if !dates.is_empty() {
            //println!("Dates: {:#?}", dates[0].parse::<DateTime<Utc>>());
            return dates[0].parse::<DateTime<FixedOffset>>().ok();
        }
    }

    None
}
