use maud::{html, Markup};

pub fn render(active_page: &str) -> Markup {
    html! {
        header {
            div class="header" {
                span class="header-logo" { "Homelab" }
            }
            div class="subheader" {
                a href="/" class="subheader-brand" {
                    "Lerke"
                }
                div class="subheader-nav" {
                    a href="/incidents" class=(if active_page == "incidents" { "subheader-nav-item active" } else { "subheader-nav-item" }) { "Incidents" }
                    a href="/debug/webhooks" class=(if active_page == "debug" { "subheader-nav-item active" } else { "subheader-nav-item" }) { "Debug" }
                }
            }
        }
    }
}

pub fn stylesheet_link() -> Markup {
    html! {
        link rel="stylesheet" href="/static/styles.css";
    }
}

pub fn scripts() -> Markup {
    html! {
        script src="/static/htmx.min.js" {}
        script src="/static/idiomorph.min.js" {}
        script src="/static/idiomorph-ext.min.js" {}
    }
}
