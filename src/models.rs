use serde::Serialize;

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BookMetadata {
    pub title: String,
    pub subtitle: Option<String>,
    pub authors: Vec<String>,
    pub narrators: Vec<String>,
    pub series: Vec<String>,
    pub genres: Vec<String>,
    pub published_year: Option<String>,
    pub published_date: Option<String>,
    pub publisher: Option<String>,
    pub description: Option<String>,
    pub isbn: Option<String>,
    pub asin: Option<String>,
    pub language: Option<String>,
    pub explicit: bool,
    pub abridged: bool,
    pub episodes: Vec<Episode>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Episode {
    pub id: usize,
    pub filename: String,
    pub duration: f64,
    pub title: String,
}
