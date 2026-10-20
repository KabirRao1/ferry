//! Search in the shown response body (F06-AC8) and the `JSONPath` filter
//! of the Pretty view (F22-AC4). Both run off the UI thread as effects;
//! each request carries a job id so a late answer for an older query is
//! ignored.

use std::sync::Arc;

use bytes::Bytes;
use ferry_core::json::{self, MAX_FILTER_BODY};
use ferry_core::text::{self, MAX_MATCHES};

use crate::app::{App, Effect, Message, WRAP_BYTES};
use crate::body_view::BodyView;
use crate::tab::{BodyFormat, Phase, Received, TabId};

/// Shown instead of the filter field for larger bodies (F22-AC4).
pub const FILTER_TOO_LARGE: &str = "Filtering is available for bodies up to 10 MB";
/// The find bar's text when nothing matches (F06-AC8).
pub const NO_RESULTS: &str = "No results";

// CONTRACT
/// The find bar's state (F06-AC8).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Find {
    pub query: String,
    /// Byte offsets into the shown body, ascending; empty while searching.
    pub matches: Vec<usize>,
    /// Counting stopped at [`MAX_MATCHES`].
    pub capped: bool,
    /// Index into `matches` of the current match.
    pub current: usize,
    /// A search for `query` is running.
    pub searching: bool,
    pub(crate) job: u64,
}

impl Find {
    // CONTRACT
    /// `<i> of <n>` (`10000+` when capped), `No results`, or nothing for an
    /// empty query or while searching.
    #[must_use]
    pub fn label(&self) -> String {
        if self.query.is_empty() || self.searching {
            String::new()
        } else if self.matches.is_empty() {
            NO_RESULTS.to_owned()
        } else {
            format!(
                "{} of {}{}",
                self.current + 1,
                self.matches.len(),
                if self.capped { "+" } else { "" }
            )
        }
    }

    // CONTRACT
    /// The current match's index, if there are matches.
    #[must_use]
    pub fn current(&self) -> Option<usize> {
        (!self.matches.is_empty()).then_some(self.current)
    }
}

// CONTRACT
/// What the `JSONPath` filter shows (F22-AC4).
#[derive(Debug, Clone, Default)]
pub enum FilterState {
    /// No expression: the body is shown.
    #[default]
    Off,
    Running,
    /// The results, pretty-printed.
    Shown(BodyView),
    /// `Invalid JSONPath: <reason>`.
    Invalid(String),
}

// CONTRACT
/// The Pretty view's `JSONPath` filter (F22-AC4).
#[derive(Debug, Clone, Default)]
pub struct Filter {
    pub expression: String,
    pub state: FilterState,
    pub(crate) job: u64,
}

impl Received {
    // CONTRACT
    /// Whether the Pretty view offers the filter field: the body is JSON
    /// and at most [`MAX_FILTER_BODY`] bytes (F22-AC4).
    #[must_use]
    pub fn filter_available(&self) -> bool {
        self.pretty.is_some() && self.response.body.len() <= MAX_FILTER_BODY
    }

    // CONTRACT
    /// Whether the find bar can search the shown body: Pretty or Raw, not
    /// Hex (F06-AC8).
    #[must_use]
    pub fn searchable(&self) -> bool {
        !matches!(self.format, BodyFormat::Preview | BodyFormat::Visualize) && self.view().is_text()
    }
}

// CONTRACT
/// Run a search or filter effect; `None` for any other effect. It blocks,
/// so the UI runs it on a worker thread.
#[must_use]
pub fn run(effect: Effect) -> Option<Message> {
    match effect {
        Effect::Find {
            tab,
            job,
            body,
            query,
        } => {
            let (matches, capped) = text::find_all(&body, &query, MAX_MATCHES);
            Some(Message::Found {
                tab,
                job,
                matches: Arc::new(matches),
                capped,
            })
        }
        Effect::Filter {
            tab,
            job,
            body,
            expression,
        } => {
            let result = json::filter(&body, &expression).and_then(|bytes| {
                BodyView::new(Bytes::from(bytes), WRAP_BYTES).map_err(|e| e.to_string())
            });
            Some(Message::Filtered {
                tab,
                job,
                result: Arc::new(result),
            })
        }
        _ => None,
    }
}

impl App {
    pub(crate) fn find_message(&mut self, message: Message) -> Option<Effect> {
        match message {
            Message::FindOpen => {
                let received = self.received_mut()?;
                if received.searchable() && received.find.is_none() {
                    received.find = Some(Find::default());
                }
                None
            }
            Message::FindClose => {
                self.received_mut()?.find = None;
                None
            }
            Message::FindQuery(query) => {
                let received = self.received_mut()?;
                let find = received.find.as_mut()?;
                find.query = query;
                self.search_active()
            }
            Message::FindNext => {
                self.step(1);
                None
            }
            Message::FindPrevious => {
                self.step(-1);
                None
            }
            Message::Found {
                tab,
                job,
                matches,
                capped,
            } => {
                let received = self.received_in(tab)?;
                let find = received.find.as_mut().filter(|f| f.job == job)?;
                find.matches = Arc::try_unwrap(matches).unwrap_or_else(|m| (*m).clone());
                find.capped = capped;
                find.searching = false;
                // Start at the first match at or below the top line.
                let top = received.view().top_offset();
                let find = received.find.as_mut()?;
                find.current = find.matches.iter().position(|&m| m >= top).unwrap_or(0);
                reveal(received);
                None
            }
            Message::FilterChanged(expression) => {
                let job = self.next_job();
                let tab = self.tabs[self.active].id;
                let received = self.received_mut()?;
                if !received.filter_available() {
                    return None;
                }
                received.filter.expression = expression.clone();
                received.filter.job = job;
                if expression.trim().is_empty() {
                    received.filter.state = FilterState::Off;
                    return self.search_active();
                }
                received.filter.state = FilterState::Running;
                Some(Effect::Filter {
                    tab,
                    job,
                    body: received.response.body.clone(),
                    expression,
                })
            }
            Message::Filtered { tab, job, result } => {
                let received = self.received_in(tab)?;
                if received.filter.job != job {
                    return None;
                }
                received.filter.state = match &*result {
                    Ok(view) => {
                        let mut view = view.clone();
                        let rows = received.pretty.as_ref().map_or(0, BodyView::rows);
                        view.update(crate::body_view::Message::Resized(rows));
                        FilterState::Shown(view)
                    }
                    Err(error) => FilterState::Invalid(error.clone()),
                };
                if received.format == BodyFormat::Pretty {
                    self.search_in(tab)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// The active tab's response, if it has one.
    fn received_mut(&mut self) -> Option<&mut Received> {
        match &mut self.tabs[self.active].phase {
            Phase::Received(received) => Some(&mut **received),
            _ => None,
        }
    }

    fn received_in(&mut self, tab: TabId) -> Option<&mut Received> {
        let i = self.index_of(tab)?;
        match &mut self.tabs[i].phase {
            Phase::Received(received) => Some(&mut **received),
            _ => None,
        }
    }

    fn next_job(&mut self) -> u64 {
        self.next_job += 1;
        self.next_job
    }

    pub(crate) fn search_active(&mut self) -> Option<Effect> {
        let tab = self.tabs[self.active].id;
        self.search_in(tab)
    }

    /// Search the shown body of `tab` again, after its query or shown text
    /// changed. An empty query clears the matches; a hex view closes the bar.
    pub(crate) fn search_in(&mut self, tab: TabId) -> Option<Effect> {
        let job = self.next_job();
        let received = self.received_in(tab)?;
        if !received.searchable() {
            received.find = None;
            return None;
        }
        let body = received.view().body().clone();
        let find = received.find.as_mut()?;
        find.matches.clear();
        find.capped = false;
        find.current = 0;
        find.job = job;
        find.searching = !find.query.is_empty();
        if !find.searching {
            return None;
        }
        Some(Effect::Find {
            tab,
            job,
            body,
            query: find.query.clone(),
        })
    }

    /// Move to the next (`1`) or previous (`-1`) match, wrapping around.
    fn step(&mut self, by: isize) {
        let Some(received) = self.received_mut() else {
            return;
        };
        let Some(find) = received.find.as_mut() else {
            return;
        };
        let n = find.matches.len();
        if n == 0 {
            return;
        }
        find.current = (find.current + n).saturating_add_signed(by) % n;
        reveal(received);
    }
}

/// Scroll the current match into view.
fn reveal(received: &mut Received) {
    let Some(offset) = received
        .find
        .as_ref()
        .and_then(|f| f.current().map(|i| f.matches[i]))
    else {
        return;
    };
    received.view_mut().reveal(offset);
}
