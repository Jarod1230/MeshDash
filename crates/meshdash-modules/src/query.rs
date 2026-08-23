//! Query parameters several modules share.
//!
//! Not domain knowledge and not a module: paging and time ranges are the same
//! mechanics wherever a listing appears, and three copies of them would drift
//! apart. Modules stay ignorant of each other — they only reach for the same
//! helper.

use chrono::{DateTime, Utc};
use serde::Deserialize;

/// The stretch of time a listing should cover.
///
/// Both ends are optional and inclusive: `since` alone means "everything from
/// there on", `until` alone means "everything up to there".
#[derive(Debug, Deserialize, Default, PartialEq)]
pub struct TimeRange {
    /// Oldest moment to include, RFC 3339.
    since: Option<String>,
    /// Newest moment to include, RFC 3339.
    until: Option<String>,
}

/// What is wrong with a time range a request asked for.
#[derive(Debug, PartialEq)]
pub struct BadTimeRange {
    /// Which parameter, so the message can name it.
    pub field: &'static str,
    /// What the request sent, echoed back so the sender sees their own value.
    pub value: String,
}

impl axum::response::IntoResponse for BadTimeRange {
    fn into_response(self) -> axum::response::Response {
        // The sender's own value goes back to them: "not a time" without
        // naming the parameter is guesswork on the other end.
        (
            axum::http::StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({
                "error": {
                    "code": "invalid_parameter",
                    "message": format!(
                        "{} is not an RFC 3339 timestamp: {}",
                        self.field, self.value
                    )
                }
            })),
        )
            .into_response()
    }
}

/// A time range in the exact shape the stored timestamps have.
///
/// Comparing timestamps as text only holds while both sides are written the
/// same way. Everything stored goes through `Utc::now().to_rfc3339()`, so
/// whatever a request sends is parsed and written out the same way rather than
/// compared as it arrived — `…T20:00:00Z` and `…T20:00:00+00:00` are the same
/// moment and different strings.
#[derive(Debug, Default, PartialEq)]
pub struct Bounds {
    /// Oldest moment to include, as stored.
    pub since: Option<String>,
    /// Newest moment to include, as stored.
    pub until: Option<String>,
}

impl TimeRange {
    /// Reads both ends, or says which one could not be read.
    pub fn bounds(&self) -> Result<Bounds, BadTimeRange> {
        Ok(Bounds {
            since: parse(self.since.as_deref(), "since")?,
            until: parse(self.until.as_deref(), "until")?,
        })
    }
}

fn parse(value: Option<&str>, field: &'static str) -> Result<Option<String>, BadTimeRange> {
    match value {
        None => Ok(None),
        Some(text) => DateTime::parse_from_rfc3339(text)
            .map(|time| Some(time.with_timezone(&Utc).to_rfc3339()))
            .map_err(|_| BadTimeRange {
                field,
                value: text.to_owned(),
            }),
    }
}

/// Everything a listing request narrows itself with.
///
/// One type rather than three parameters per read function: paging and time
/// bounds always travel together, and a call site with four positional
/// `Option`s next to each other is one swapped pair away from a silent bug.
#[derive(Debug, PartialEq)]
pub struct Window {
    /// How many rows at most.
    pub limit: i64,
    /// Only rows older than this id — the cursor.
    pub before: Option<i64>,
    /// Oldest moment to include, written as the database stores it.
    pub since: Option<String>,
    /// Newest moment to include, written as the database stores it.
    pub until: Option<String>,
}

impl Window {
    /// A window that only pages, without time bounds.
    pub fn paged(limit: i64, before: Option<i64>) -> Self {
        Self {
            limit,
            before,
            since: None,
            until: None,
        }
    }

    /// A window that pages within a time range.
    pub fn new(limit: i64, before: Option<i64>, bounds: Bounds) -> Self {
        Self {
            limit,
            before,
            since: bounds.since,
            until: bounds.until,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(since: Option<&str>, until: Option<&str>) -> TimeRange {
        TimeRange {
            since: since.map(str::to_owned),
            until: until.map(str::to_owned),
        }
    }

    #[test]
    fn an_absent_range_bounds_nothing() {
        assert_eq!(range(None, None).bounds(), Ok(Bounds::default()));
    }

    #[test]
    fn writes_both_ends_the_way_the_database_stores_them() {
        // Same moment, three spellings a client might send.
        for sent in [
            "2026-08-22T20:00:00Z",
            "2026-08-22T20:00:00+00:00",
            "2026-08-22T22:00:00+02:00",
        ] {
            let bounds = range(Some(sent), None).bounds().unwrap();
            assert_eq!(
                bounds.since.as_deref(),
                Some("2026-08-22T20:00:00+00:00"),
                "sent as {sent}"
            );
        }
    }

    #[test]
    fn a_stored_timestamp_sorts_after_an_earlier_bound_as_text() {
        // The whole approach rests on this: the comparison happens in SQL, as
        // text. Fractional seconds are written only when they are not zero,
        // and that must not flip the order.
        let bound = range(Some("2026-08-22T20:00:00Z"), None)
            .bounds()
            .unwrap()
            .since
            .unwrap();
        let stored = "2026-08-22T20:00:00.123456+00:00";

        assert!(stored > bound.as_str());
        assert!("2026-08-22T19:59:59.9+00:00" < bound.as_str());
    }

    #[test]
    fn an_unreadable_range_answers_400_and_names_the_parameter() {
        use axum::response::IntoResponse;

        let response = range(Some("vorgestern"), None)
            .bounds()
            .unwrap_err()
            .into_response();

        assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn says_which_end_could_not_be_read() {
        assert_eq!(
            range(None, Some("gestern")).bounds(),
            Err(BadTimeRange {
                field: "until",
                value: "gestern".into()
            })
        );
    }
}
