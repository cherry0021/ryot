use anyhow::{anyhow, Result};
use async_graphql::SimpleObject;
use async_trait::async_trait;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use surf::{http::headers::USER_AGENT, Client, Config, Url};

use crate::{
    config::AudibleConfig,
    graphql::{AUTHOR, PROJECT_NAME},
    migrator::{MetadataImageLot, MetadataLot, MetadataSource},
    miscellaneous::{
        resolver::{MediaDetails, MediaSearchItem, MediaSearchResults},
        MediaSpecifics, MetadataCreator, MetadataImage, MetadataImageUrl, PAGE_LIMIT,
    },
    models::media::AudioBookSpecifics,
    traits::{MediaProvider, MediaProviderLanguages},
    utils::{convert_date_to_year, convert_string_to_date, NamedObject},
};

pub static LOCALES: [&str; 10] = ["au", "ca", "de", "es", "fr", "in", "it", "jp", "gb", "us"];

#[derive(Serialize, Deserialize)]
struct PrimaryQuery {
    response_groups: String,
    image_sizes: String,
}

impl Default for PrimaryQuery {
    fn default() -> Self {
        Self {
            response_groups: [
                "contributors",
                "category_ladders",
                "media",
                "product_attrs",
                "product_extended_attrs",
            ]
            .join(","),
            image_sizes: ["2400"].join(","),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct SearchQuery {
    title: String,
    num_results: i32,
    page: i32,
    products_sort_by: String,
    #[serde(flatten)]
    primary: PrimaryQuery,
}

#[derive(Debug, Serialize, Deserialize, SimpleObject)]
pub struct AudiblePoster {
    #[serde(rename = "2400")]
    image: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, SimpleObject)]
pub struct AudibleCategoryLadderCollection {
    ladder: Vec<NamedObject>,
}

#[derive(Debug, Serialize, Deserialize, SimpleObject)]
pub struct AudibleItem {
    asin: String,
    title: String,
    authors: Option<Vec<NamedObject>>,
    narrators: Option<Vec<NamedObject>>,
    product_images: AudiblePoster,
    merchandising_summary: Option<String>,
    publisher_summary: Option<String>,
    release_date: Option<String>,
    runtime_length_min: Option<i32>,
    category_ladders: Option<Vec<AudibleCategoryLadderCollection>>,
}

#[derive(Debug, Clone)]
pub struct AudibleService {
    client: Client,
}

impl MediaProviderLanguages for AudibleService {
    fn supported_languages() -> Vec<String> {
        LOCALES.into_iter().map(String::from).collect()
    }

    fn default_language() -> String {
        "us".to_owned()
    }
}

impl AudibleService {
    fn url_from_locale(locale: &str) -> String {
        let suffix = match locale {
            "us" => "com",
            "ca" => "ca",
            "uk" => "co.uk",
            "au" => "co.au",
            "fr" => "fr",
            "de" => "de",
            "jp" => "co.jp",
            "it" => "it",
            "in" => "co.in",
            "es" => "es",
            _ => unreachable!(),
        };
        format!("https://api.audible.{}/1.0/catalog/products/", suffix)
    }

    pub fn new(config: &AudibleConfig) -> Self {
        let url = Self::url_from_locale(&config.locale);
        let client = Config::new()
            .add_header(USER_AGENT, format!("{}/{}", AUTHOR, PROJECT_NAME))
            .unwrap()
            .set_base_url(Url::parse(&url).unwrap())
            .try_into()
            .unwrap();
        Self { client }
    }
}

#[async_trait]
impl MediaProvider for AudibleService {
    async fn details(&self, identifier: &str) -> Result<MediaDetails> {
        #[derive(Serialize, Deserialize, Debug)]
        struct AudibleItemResponse {
            product: AudibleItem,
        }
        let mut rsp = self
            .client
            .get(identifier)
            .query(&PrimaryQuery::default())
            .unwrap()
            .await
            .map_err(|e| anyhow!(e))?;
        let data: AudibleItemResponse = rsp.body_json().await.map_err(|e| anyhow!(e))?;
        let d = self.audible_response_to_search_response(data.product);
        Ok(d)
    }

    async fn search(&self, query: &str, page: Option<i32>) -> Result<MediaSearchResults> {
        let page = page.unwrap_or(1);
        #[derive(Serialize, Deserialize, Debug)]
        struct AudibleSearchResponse {
            total_results: i32,
            products: Vec<AudibleItem>,
        }
        let mut rsp = self
            .client
            .get("")
            .query(&SearchQuery {
                title: query.to_owned(),
                num_results: PAGE_LIMIT,
                page: page - 1,
                products_sort_by: "Relevance".to_owned(),
                primary: PrimaryQuery::default(),
            })
            .unwrap()
            .await
            .map_err(|e| anyhow!(e))?;
        let search: AudibleSearchResponse = rsp.body_json().await.map_err(|e| anyhow!(e))?;
        let resp = search
            .products
            .into_iter()
            .map(|d| {
                let a = self.audible_response_to_search_response(d);
                MediaSearchItem {
                    identifier: a.identifier,
                    lot: MetadataLot::AudioBook,
                    title: a.title,
                    images: a
                        .images
                        .into_iter()
                        .map(|i| match i.url {
                            MetadataImageUrl::S3(_u) => unreachable!(),
                            MetadataImageUrl::Url(u) => u,
                        })
                        .collect(),
                    publish_year: a.publish_year,
                }
            })
            .collect::<Vec<_>>();
        let next_page = if search.total_results - ((page) * PAGE_LIMIT) > 0 {
            Some(page + 1)
        } else {
            None
        };
        Ok(MediaSearchResults {
            total: search.total_results,
            items: resp,
            next_page,
        })
    }
}

impl AudibleService {
    fn audible_response_to_search_response(&self, item: AudibleItem) -> MediaDetails {
        let images = Vec::from_iter(item.product_images.image.map(|a| MetadataImage {
            url: MetadataImageUrl::Url(a),
            lot: MetadataImageLot::Poster,
        }));
        let release_date = item.release_date.unwrap_or_default();
        let mut creators = item
            .authors
            .unwrap_or_default()
            .into_iter()
            .map(|a| MetadataCreator {
                name: a.name,
                role: "Author".to_owned(),
                image_urls: vec![],
            })
            .collect::<Vec<_>>();
        creators.extend(
            item.narrators
                .unwrap_or_default()
                .into_iter()
                .map(|a| MetadataCreator {
                    name: a.name,
                    role: "Narrator".to_owned(),
                    image_urls: vec![],
                }),
        );
        let description = item.publisher_summary.or(item.merchandising_summary);
        MediaDetails {
            identifier: item.asin,
            lot: MetadataLot::AudioBook,
            source: MetadataSource::Audible,
            title: item.title,
            description,
            creators,
            genres: item
                .category_ladders
                .unwrap_or_default()
                .into_iter()
                .flat_map(|c| c.ladder.into_iter().map(|l| l.name))
                .unique()
                .collect(),
            publish_year: convert_date_to_year(&release_date),
            publish_date: convert_string_to_date(&release_date),
            specifics: MediaSpecifics::AudioBook(AudioBookSpecifics {
                runtime: item.runtime_length_min,
            }),
            images,
        }
    }
}
