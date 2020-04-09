use log::*;
use serde_derive::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use strum_macros::{EnumIter, ToString};
use yew::format::Json;
use yew::services::storage::{Area, StorageService};
use yew::prelude::*;

use std::path::PathBuf;

const KEY: &str = "be4k.news.self";

pub struct App {
    link: ComponentLink<Self>,
    storage: StorageService,
    state: State,
}

#[derive(Serialize, Deserialize)]
pub struct State {
    entries: Vec<Entry>,
    filter: Filter,
}

#[derive(Serialize, Deserialize)]
struct Entry {
    item: rss::Item,
    pub image_path: Option<PathBuf>,
    pub_date: Option<chrono::DateTime<chrono::FixedOffset>>,
    read: bool,
}

pub enum Msg {
    Read(usize),
    SetFilter(Filter),
    Nope,
}

impl Component for App {
    type Message = Msg;
    type Properties = ();

    fn create(_: Self::Properties, link: ComponentLink<Self>) -> Self {
        let storage = StorageService::new(Area::Local).unwrap();
        let entries = {
            if let Json(Ok(restored_entries)) = storage.restore(KEY) {
                restored_entries
            } else {
                Vec::new()
            }
        };
        let state = State {
            entries,
            filter: Filter::All,
        };
        App { link, storage, state }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::Read(idx) => {
                self.state.read(idx);
            }
            Msg::SetFilter(filter) => {
                self.state.filter = filter;
            }
            Msg::Nope => {}
        }
        self.storage.store(KEY, Json(&self.state.entries));
        true
    }

    fn view(&self) -> Html {
        info!("rendered!");
        html! {
            <div class="news-wrapper">
                <section class="newsapp">
                    <header class="header">
                        <h1>{ "news" }</h1>
                    </header>
                    <section class="main">
                        <ul class="news-list">
                            { for self.state.entries.iter().filter(|e| self.state.filter.fit(e))
                                .enumerate()
                                .map(|val| self.view_entry(val)) }
                        </ul>
                    </section>
                    <footer class="footer">
                        <span class="unread">
                            <strong>{ self.state.total_unread() }</strong>
                            { " item(s) left" }
                        </span>
                        <ul class="filters">
                            { for Filter::iter().map(|flt| self.view_filter(flt)) }
                        </ul>
                    </footer>
                </section>
                <footer class="info">
                    <p>{ "Written by " }<a href="https://github.com/BrandonEdens/" target="_blank">{ "Brandon Edens" }</a></p>
                </footer>
            </div>
        }
    }
}


impl App {
    fn view_filter(&self, filter: Filter) -> Html {
        let flt = filter.clone();

        html! {
            <li>
                <a class=if self.state.filter == flt { "selected" } else { "not-selected" }
                   href=&flt
                   onclick=self.link.callback(move |_| Msg::SetFilter(flt.clone()))>
                    { filter }
                </a>
            </li>
        }
    }

    fn view_entry(&self, (idx, entry): (usize, &Entry)) -> Html {
        let mut class = "news".to_string();
        html! {
            <li class=class>
                { self.view_entry((idx, &entry)) }
            </li>
        }
    }
}



#[derive(EnumIter, ToString, Clone, PartialEq, Serialize, Deserialize)]
pub enum Filter {
    All,
    Read,
    Unread,
}

impl<'a> Into<Href> for &'a Filter {
    fn into(self) -> Href {
        match *self {
            Filter::All => "#/".into(),
            Filter::Read => "#/read".into(),
            Filter::Unread => "#/unread".into(),
        }
    }
}

impl Filter {
    fn fit(&self, entry: &Entry) -> bool {
        match *self {
            Filter::All => true,
            Filter::Unread => !entry.read,
            Filter::Read => entry.read,
        }
    }
}

impl State {
    fn read(&mut self, idx: usize) {
        self.entries[idx].read = true;
    }

    fn total(&self) -> usize {
        self.entries.len()
    }

    fn total_read(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| Filter::Read.fit(e))
            .count()
    }

    fn total_unread(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| Filter::Unread.fit(e))
            .count()
    }

    fn is_all_read(&self) -> bool {
        let mut filtered_iter = self
            .entries
            .iter()
            .filter(|e| self.filter.fit(e))
            .peekable();

        if filtered_iter.peek().is_none() {
            return false;
        }

        filtered_iter.all(|e| e.read)
    }
}
