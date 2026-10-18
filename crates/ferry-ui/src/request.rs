//! The request builder: method, URL and Send, then the Params,
//! Authorization, Headers, Body, Scripts and Settings tabs (F01..F05, F14-AC2).

use ferry_app::{
    ApiKeyLocation, App, AuthDraft, AuthKind, BodyMode, Edit, FileTarget, KeyValue,
    Message as AppMessage, Method, RawLanguage, RequestTab, RowEdit, Tab, Table, VarState,
    collection,
};
use iced::widget::text::Wrapping;
use iced::widget::{
    button, checkbox, column, combo_box, container, pick_list, radio, row, scrollable, space, text,
    text_editor, text_input, toggler, tooltip,
};
use iced::{Alignment, Fill, Font, Length};

use crate::editors::Editors;
use crate::screen::{SMALL, TEXT, coming};
use crate::shell::Message;
use crate::{Element, theme};

/// Id of the URL field, for focus and tests.
pub const URL_INPUT: &str = "url";
/// Placeholder shown in the empty URL field.
pub const URL_PLACEHOLDER: &str = "Enter URL or paste text";
pub const NO_BODY: &str = "This request does not have a body";
pub const NO_AUTH: &str = "This request does not use any authorization.";

fn edit(e: Edit) -> Message {
    Message::App(AppMessage::Edit(e))
}

fn app(m: AppMessage) -> Message {
    Message::App(m)
}

/// Shown for secret variable values (F08-AC6).
pub const MASKED: &str = "••••••";

/// The request builder for `tab`.
#[must_use]
pub fn view<'a>(app_state: &'a App, tab: &'a Tab, editors: &'a Editors) -> Element<'a, Message> {
    let content: Element<'a, Message> = match tab.request_tab {
        RequestTab::Params => table(tab, editors, Table::Params, "Query Params"),
        RequestTab::Authorization => {
            let note = (tab.draft.auth.kind == AuthKind::Inherit)
                .then(|| collection::inherit_note(app_state.inherited_auth(tab).as_ref()));
            auth_view(&tab.draft.auth, note)
        }
        RequestTab::Headers => headers(tab, editors),
        RequestTab::Body => body(tab, editors),
        RequestTab::Scripts => coming(
            "Scripts arrive in Phase 4",
            "Pre-request and post-response JavaScript with the pm API: set variables, chain requests and write tests.",
        ),
        RequestTab::Settings => settings(tab),
    };
    let mut page = column![title_row(app_state, tab), url_bar(tab, editors)].spacing(10);
    if let Some(strip) = variable_strip(app_state) {
        page = page.push(strip);
    }
    container(
        page.push(sub_tabs(tab))
            .push(container(content).height(Fill).width(Fill)),
    )
    .padding([10, 16])
    .height(Fill)
    .style(theme::surface)
    .into()
}

/// The request's location and the Save button (F14-AC6).
fn title_row<'a>(app_state: &'a App, tab: &'a Tab) -> Element<'a, Message> {
    let location = app_state.location(tab);
    let mut crumbs = row![].spacing(6).align_y(Alignment::Center);
    if location.is_empty() {
        crumbs = crumbs.push(text(tab.title()).size(14).wrapping(Wrapping::None));
    } else {
        let last = location.len() - 1;
        for (i, part) in location.into_iter().enumerate() {
            if i > 0 {
                crumbs = crumbs.push(text("/").size(14).style(theme::faint));
            }
            let label = text(part).size(14).wrapping(Wrapping::None);
            crumbs = crumbs.push(if i == last {
                label
            } else {
                label.style(theme::muted)
            });
        }
    }
    row![
        container(crumbs).width(Fill).clip(true),
        button(text("Save").size(TEXT))
            .on_press(app(AppMessage::Save))
            .padding([5, 14])
            .style(theme::secondary),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

/// The variables the request uses, as chips under the URL bar (F08-AC6).
fn variable_strip(app_state: &App) -> Option<Element<'_, Message>> {
    let uses = app_state.variable_strip();
    if uses.is_empty() {
        return None;
    }
    let chips = uses.into_iter().map(|u| {
        let (value, detail, missing) = match u.state {
            VarState::Defined {
                value,
                scope,
                secret,
            } => {
                let shown = if secret { MASKED.to_owned() } else { value };
                (shown, scope.to_string(), false)
            }
            VarState::Dynamic => (
                "dynamic".to_owned(),
                "Generated when sent".to_owned(),
                false,
            ),
            VarState::Missing => (
                "not defined".to_owned(),
                "Not in the environment, collection or globals; sending is blocked".to_owned(),
                true,
            ),
        };
        let chip = container(
            row![
                text(format!("{{{{{}}}}}", u.name))
                    .size(SMALL)
                    .style(if missing { theme::error } else { theme::accent }),
                text(value)
                    .size(SMALL)
                    .style(if missing { theme::error } else { theme::muted })
                    .wrapping(Wrapping::None),
            ]
            .spacing(6),
        )
        .padding([2, 8])
        .max_width(320)
        .clip(true)
        .style(if missing {
            theme::chip_error
        } else {
            theme::chip
        });
        tooltip(
            chip,
            container(text(detail).size(SMALL))
                .padding(6)
                .style(theme::dialog),
            tooltip::Position::Bottom,
        )
        .into()
    });
    Some(row(chips).spacing(6).wrap().into())
}

fn url_bar<'a>(tab: &'a Tab, editors: &'a Editors) -> Element<'a, Message> {
    let method: Element<'a, Message> = match editors.get(tab.id) {
        Some(e) => combo_box(&e.method, "Method", Some(&tab.draft.method), |m: Method| {
            edit(Edit::MethodSelected(m))
        })
        .on_input(|t| edit(Edit::MethodText(t)))
        .size(TEXT)
        .padding([8, 10])
        .width(130)
        .input_style(theme::input)
        .into(),
        None => text(tab.draft.method.as_str()).into(),
    };
    let url = text_input(URL_PLACEHOLDER, &tab.draft.url)
        .id(iced::widget::Id::new(URL_INPUT))
        .on_input(|u| edit(Edit::Url(u)))
        .on_submit(app(AppMessage::SendPressed))
        .size(TEXT)
        .padding([8, 10])
        .width(Fill)
        .style(theme::input);
    let action = if tab.is_sending() {
        button(text("Cancel").size(TEXT).align_x(Alignment::Center))
            .on_press(app(AppMessage::CancelPressed))
            .style(theme::secondary)
    } else {
        button(text("Send").size(TEXT).align_x(Alignment::Center))
            .on_press(app(AppMessage::SendPressed))
            .style(theme::primary)
    };
    let mut bar = column![
        row![method, url, action.padding([8, 22]).width(96)]
            .spacing(6)
            .align_y(Alignment::Center)
    ];
    if let Some(error) = &tab.method_error {
        bar = bar.push(text(error.to_string()).size(SMALL).style(theme::error));
    }
    bar.spacing(4).into()
}

/// Request tab labels with counts and dots (F14-AC2).
#[must_use]
pub fn tab_label(tab: &Tab, which: RequestTab) -> String {
    let d = &tab.draft;
    match which {
        RequestTab::Params => match d.params.iter().filter(|p| p.row.is_active()).count() {
            0 => "Params".into(),
            n => format!("Params ({n})"),
        },
        RequestTab::Authorization if d.auth.kind != AuthKind::None => "Authorization ●".into(),
        RequestTab::Authorization => "Authorization".into(),
        RequestTab::Headers => {
            let user = d.active_headers().count();
            let auto = d.auto_headers().iter().filter(|a| !a.overridden).count();
            format!("Headers ({})", user + auto)
        }
        RequestTab::Body if d.body.mode != BodyMode::None => "Body ●".into(),
        RequestTab::Body => "Body".into(),
        RequestTab::Scripts => "Scripts".into(),
        RequestTab::Settings => "Settings".into(),
    }
}

fn sub_tabs(tab: &Tab) -> Element<'_, Message> {
    let tabs = row(RequestTab::ALL.into_iter().map(|which| {
        let selected = which == tab.request_tab;
        column![
            button(text(tab_label(tab, which)).size(TEXT))
                .on_press(app(AppMessage::RequestTabSelected(which)))
                .padding([4, 10])
                .style(theme::sub_tab(selected)),
            underline(selected),
        ]
        .width(Length::Shrink)
        .into()
    }))
    .spacing(2);
    // The cookie manager, under Send as in Postman (F17-AC5).
    let cookies = button(text("Cookies").size(SMALL).style(theme::accent))
        .on_press(app(AppMessage::OpenCookies))
        .padding([4, 8])
        .style(theme::flat(false));
    row![tabs, space().width(Fill), cookies]
        .align_y(Alignment::Center)
        .into()
}

/// The 2 px line under the selected sub-tab.
pub fn underline<'a>(selected: bool) -> Element<'a, Message> {
    if selected {
        container(space().height(2).width(Fill))
            .style(theme::accent_line)
            .into()
    } else {
        space().height(2).into()
    }
}

// --- tables -----------------------------------------------------------------------

/// A key-value table with bulk edit (F01-AC8, F02-AC1, F02-AC4).
fn table<'a>(
    tab: &'a Tab,
    editors: &'a Editors,
    which: Table,
    title: &'a str,
) -> Element<'a, Message> {
    table_with(tab, editors, which, title, None, None)
}

/// A table whose toolbar has an `extra` control and whose scrolling area
/// starts with `prefix` (the Headers tab's auto-generated headers).
fn table_with<'a>(
    tab: &'a Tab,
    editors: &'a Editors,
    which: Table,
    title: &'a str,
    extra: Option<Element<'a, Message>>,
    prefix: Option<Element<'a, Message>>,
) -> Element<'a, Message> {
    let in_bulk = tab.bulk_text(which).is_some();
    let toolbar = row![
        text(title).size(SMALL).style(theme::muted),
        extra.unwrap_or_else(|| space().width(0).into()),
        space().width(Fill),
        button(
            text(if in_bulk {
                "Key-Value Edit"
            } else {
                "Bulk Edit"
            })
            .size(SMALL)
        )
        .on_press(edit(Edit::ToggleBulk(which)))
        .padding([2, 8])
        .style(theme::flat(false)),
    ]
    .align_y(Alignment::Center);
    let content: Element<'a, Message> = match editors.bulk(tab.id, which) {
        Some(content) if in_bulk => text_editor(content)
            .on_action(move |a| Message::BulkAction(which, a))
            .placeholder("Rows are separated by new lines\nKeys and values are separated by :\nPrepend // to any row you want to add but keep disabled")
            .font(Font::MONOSPACE)
            .size(TEXT)
            .height(Fill)
            .into(),
        _ => match prefix {
            Some(prefix) => scrollable(column![prefix, rows(tab, which)].spacing(4))
                .height(Fill)
                .into(),
            None => scrollable(rows(tab, which)).height(Fill).into(),
        },
    };
    column![toolbar, content].spacing(6).into()
}

fn rows(tab: &Tab, which: Table) -> Element<'_, Message> {
    let d = &tab.draft;
    let data: Vec<(&KeyValue, Option<&std::path::Path>)> = match which {
        Table::Params => d.params.iter().map(|p| (&p.row, None)).collect(),
        Table::Headers => d.headers.iter().map(|r| (r, None)).collect(),
        Table::UrlEncoded => d.body.urlencoded.iter().map(|r| (r, None)).collect(),
        Table::Form => d
            .body
            .form
            .iter()
            .map(|f| (&f.row, f.file.as_deref()))
            .collect(),
        Table::ExampleHeaders => Vec::new(),
    };
    table_rows(which, &data)
}

/// The rows of a key-value table, with its trailing blank row (F01-AC8).
#[must_use]
pub fn table_rows<'a>(
    which: Table,
    data: &[(&KeyValue, Option<&std::path::Path>)],
) -> Element<'a, Message> {
    let is_form = which == Table::Form;
    let header = row![
        space().width(28),
        text("Key").size(SMALL).style(theme::muted).width(Fill),
        text("Value").size(SMALL).style(theme::muted).width(Fill),
        text("Description")
            .size(SMALL)
            .style(theme::muted)
            .width(Fill),
        space().width(28),
    ]
    .spacing(4)
    .padding([4, 0]);
    let blank = KeyValue {
        enabled: true,
        ..KeyValue::default()
    };
    let mut list = column![header].spacing(1);
    for (i, (row_data, file)) in data.iter().enumerate() {
        list = list.push(table_row(which, i, row_data, *file, is_form, false));
    }
    let n = data.len();
    list = list.push(table_row(which, n, &blank, None, is_form, true));
    list.into()
}

fn table_row<'a>(
    which: Table,
    i: usize,
    data: &KeyValue,
    file: Option<&std::path::Path>,
    is_form: bool,
    blank: bool,
) -> Element<'a, Message> {
    let cell = |placeholder: &str, value: &str, f: fn(String) -> RowEdit| {
        text_input(placeholder, value)
            .on_input(move |v| edit(Edit::Row(which, i, f(v))))
            .size(TEXT)
            .padding([5, 6])
            .width(Fill)
            .style(theme::cell)
    };
    let enabled: Element<'a, Message> = if blank {
        space().width(28).into()
    } else {
        container(
            checkbox(data.enabled)
                .on_toggle(move |on| edit(Edit::Row(which, i, RowEdit::Enabled(on))))
                .size(14),
        )
        .width(28)
        .align_x(Alignment::Center)
        .into()
    };
    let value: Element<'a, Message> = match file {
        Some(path) => row![
            text(path.file_name().map_or_else(
                || path.display().to_string(),
                |n| n.to_string_lossy().into_owned()
            ))
            .size(TEXT)
            .wrapping(Wrapping::None)
            .width(Fill),
            button(text("×").size(12))
                .on_press(edit(Edit::Row(which, i, RowEdit::ClearFile)))
                .padding([0, 6])
                .style(theme::flat(false)),
        ]
        .align_y(Alignment::Center)
        .padding([0, 6])
        .width(Fill)
        .into(),
        None if is_form => row![
            cell(
                if blank { "Value" } else { "" },
                &data.value,
                RowEdit::Value
            ),
            button(text("File").size(SMALL))
                .on_press(app(AppMessage::PickFile(FileTarget::FormField(i))))
                .padding([2, 6])
                .style(theme::flat(false)),
        ]
        .align_y(Alignment::Center)
        .width(Fill)
        .into(),
        None => cell(
            if blank { "Value" } else { "" },
            &data.value,
            RowEdit::Value,
        )
        .into(),
    };
    let remove: Element<'a, Message> = if blank {
        space().width(28).into()
    } else {
        button(text("×").size(14))
            .on_press(edit(Edit::Row(which, i, RowEdit::Remove)))
            .padding([0, 8])
            .width(28)
            .style(theme::flat(false))
            .into()
    };
    container(
        row![
            enabled,
            cell(if blank { "Key" } else { "" }, &data.key, RowEdit::Key),
            value,
            cell(
                if blank { "Description" } else { "" },
                &data.description,
                RowEdit::Description
            ),
            remove,
        ]
        .spacing(4)
        .align_y(Alignment::Center),
    )
    .style(theme::raised)
    .into()
}

// --- headers ------------------------------------------------------------------------

fn headers<'a>(tab: &'a Tab, editors: &'a Editors) -> Element<'a, Message> {
    let autos = tab.draft.auto_headers();
    let toggle = button(
        text(if tab.show_auto_headers {
            format!("Hide auto-generated headers ({})", autos.len())
        } else {
            format!("{} hidden", autos.len())
        })
        .size(SMALL),
    )
    .on_press(edit(Edit::ToggleAutoHeaders))
    .padding([2, 8])
    .style(theme::flat(false));
    let mut content = column![].spacing(0);
    if tab.show_auto_headers {
        let auto_rows = column(autos.into_iter().map(|a| {
            let note = if a.overridden { "overridden" } else { "auto" };
            row![
                space().width(28),
                text(a.name).size(TEXT).style(theme::muted).width(Fill),
                text(a.value)
                    .size(TEXT)
                    .style(theme::muted)
                    .wrapping(Wrapping::None)
                    .width(Fill),
                text(note).size(SMALL).style(theme::faint).width(Fill),
                space().width(28),
            ]
            .spacing(4)
            .padding([5, 0])
            .into()
        }));
        content = content.push(auto_rows);
    }
    table_with(
        tab,
        editors,
        Table::Headers,
        "Headers",
        Some(toggle.into()),
        Some(content.into()),
    )
}

// --- auth -----------------------------------------------------------------------------

/// The Authorization editor for `a`: the request's, or a collection's or
/// folder's (F04, F09-AC11). `note` explains Inherit (F04-AC6).
#[must_use]
pub fn auth_view(a: &AuthDraft, note: Option<String>) -> Element<'_, Message> {
    let kind = column![
        text("Auth Type").size(SMALL).style(theme::muted),
        pick_list(AuthKind::ALL, Some(a.kind), |k| edit(Edit::AuthKind(k)))
            .text_size(TEXT)
            .width(220),
        text("The authorization header will be generated automatically when you send the request.")
            .size(SMALL)
            .style(theme::faint),
    ]
    .spacing(6)
    .width(280);
    let field = |label: &'static str, value: &str, secure: bool, f: fn(String) -> Edit| {
        row![
            text(label).size(TEXT).width(90),
            text_input(label, value)
                .on_input(move |v| edit(f(v)))
                .secure(secure)
                .size(TEXT)
                .padding([6, 8])
                .style(theme::input),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
    };
    let fields: Element<'_, Message> = match a.kind {
        AuthKind::None => text(NO_AUTH).size(TEXT).style(theme::muted).into(),
        AuthKind::Inherit => text(note.unwrap_or_default())
            .size(TEXT)
            .style(theme::muted)
            .into(),
        AuthKind::Basic => column![
            field("Username", &a.username, false, Edit::Username),
            field("Password", &a.password, true, Edit::Password),
        ]
        .spacing(8)
        .into(),
        AuthKind::Bearer => field("Token", &a.token, false, Edit::Token).into(),
        AuthKind::ApiKey => column![
            field("Key", &a.key, false, Edit::ApiKey),
            field("Value", &a.value, false, Edit::ApiValue),
            row![
                text("Add to").size(TEXT).width(90),
                pick_list(ApiKeyLocation::ALL, Some(a.location), |l| {
                    edit(Edit::ApiLocation(l))
                })
                .text_size(TEXT),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        ]
        .spacing(8)
        .into(),
    };
    row![kind, container(fields).width(Fill).max_width(520)]
        .spacing(24)
        .into()
}

// --- body -----------------------------------------------------------------------------

fn body<'a>(tab: &'a Tab, editors: &'a Editors) -> Element<'a, Message> {
    let d = &tab.draft.body;
    let modes = row(BodyMode::ALL.into_iter().map(|mode| {
        radio(mode.label(), mode, Some(d.mode), |m| {
            edit(Edit::BodyMode(m))
        })
        .size(14)
        .text_size(TEXT)
        .into()
    }))
    .spacing(16)
    .align_y(Alignment::Center);
    let mut toolbar = row![modes].spacing(16).align_y(Alignment::Center);
    if d.mode == BodyMode::Raw {
        toolbar = toolbar.push(
            pick_list(RawLanguage::ALL, Some(d.language), |l| {
                edit(Edit::RawLanguage(l))
            })
            .text_size(SMALL)
            .padding([2, 8]),
        );
        if d.language == RawLanguage::Json {
            toolbar = toolbar.push(space().width(Fill)).push(
                button(text("Beautify").size(SMALL))
                    .on_press(edit(Edit::Beautify))
                    .padding([2, 8])
                    .style(theme::flat(false)),
            );
        }
    }
    let content: Element<'a, Message> = match d.mode {
        BodyMode::None => container(text(NO_BODY).size(TEXT).style(theme::muted))
            .center_x(Fill)
            .padding(24)
            .into(),
        BodyMode::FormData => table(tab, editors, Table::Form, "Form fields"),
        BodyMode::UrlEncoded => table(tab, editors, Table::UrlEncoded, "Form fields"),
        BodyMode::Raw => raw(tab, editors),
        BodyMode::Binary => binary(tab),
    };
    column![toolbar, content].spacing(10).into()
}

fn raw<'a>(tab: &'a Tab, editors: &'a Editors) -> Element<'a, Message> {
    let Some(e) = editors.get(tab.id) else {
        return space().into();
    };
    let editor = text_editor(&e.raw)
        .on_action(Message::RawAction)
        .font(Font::MONOSPACE)
        .size(TEXT)
        .height(Fill)
        .padding(8);
    match &tab.json_error {
        Some(error) => column![
            editor,
            text(error.to_string()).size(SMALL).style(theme::error),
        ]
        .spacing(4)
        .into(),
        None => editor.into(),
    }
}

fn binary(tab: &Tab) -> Element<'_, Message> {
    let picker = button(text("Select File").size(TEXT))
        .on_press(app(AppMessage::PickFile(FileTarget::Binary)))
        .padding([6, 12])
        .style(theme::secondary);
    match &tab.draft.body.binary {
        Some(path) => row![
            text(path.display().to_string())
                .size(TEXT)
                .wrapping(Wrapping::None),
            button(text("×").size(14))
                .on_press(edit(Edit::ClearBinary))
                .padding([0, 8])
                .style(theme::flat(false)),
            picker,
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into(),
        None => picker.into(),
    }
}

// --- settings -------------------------------------------------------------------------

fn settings(tab: &Tab) -> Element<'_, Message> {
    let o = tab.draft.options;
    let setting = |label: &'static str, detail: &'static str, on: bool, f: fn(bool) -> Edit| {
        row![
            column![
                text(label).size(TEXT),
                text(detail).size(SMALL).style(theme::muted),
            ]
            .spacing(2)
            .width(Fill),
            toggler(on).on_toggle(move |v| edit(f(v))).size(18),
        ]
        .spacing(16)
        .align_y(Alignment::Center)
        .width(Length::Fixed(720.0))
    };
    column![
        setting(
            "Automatically follow redirects",
            "Follow HTTP 3xx responses as redirects, up to 10 hops.",
            o.follow_redirects,
            Edit::FollowRedirects,
        ),
        setting(
            "Enable SSL certificate verification",
            "Verify certificates when sending a request. Verification failures abort the request.",
            o.verify_tls,
            Edit::VerifyTls,
        ),
        setting(
            "Send and store cookies",
            "Send cookies from the cookie jar with this request and store the ones it receives.",
            o.cookies,
            Edit::SendCookies,
        ),
    ]
    .spacing(18)
    .into()
}
