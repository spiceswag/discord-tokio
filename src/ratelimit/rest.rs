//! Tracking for rate-limits on discord REST clients.

use std::{collections::HashMap, sync::Mutex};

use chrono::{DateTime, Duration, NaiveDateTime, TimeZone, Utc};
use rand::{distributions::Distribution, thread_rng};
use reqwest::Response;

use crate::Error;

/// Tracking for rate-limits on discord REST clients.
/// See the [module level documentation][self] for more.
#[derive(Debug, Default)]
pub struct RateLimits {
    global: Mutex<GlobalLimitCounter>,
    routes: Mutex<HashMap<String, LimitCounter>>,
}

impl RateLimits {
    /// Check if the client is currently rate-limited, and return a
    /// sleep future that resolves once limits have elapsed, plus a random offset.
    ///
    /// The url parameter is the path part of the discord query URL.
    /// For example, `/channels/012345678910/messages` is a query URL.
    ///
    /// This function optimistically increments all affected counters.
    #[inline]
    pub fn check(&self, url: &str) -> Option<tokio::time::Sleep> {
        let mut global = self.global.lock().expect("poisoned global counter");
        match global.increment_and_check() {
            Ok(_) => {}
            Err(sleep_until) => {
                let now = Utc::now();
                let time_between = sleep_until - now;

                let random_offset = Duration::milliseconds(
                    rand::distributions::Uniform::new(0, 10).sample(&mut thread_rng()),
                );
                let time_to_sleep = time_between + random_offset;

                return Some(tokio::time::sleep(time_to_sleep.to_std().unwrap()));
            }
        }

        drop(global);

        let mut routes = self.routes.lock().expect("poisoned per-route counters");
        let route = routes
            .entry(url.to_string())
            .or_insert(LimitCounter::default());

        match route.decrement_and_check() {
            Ok(_) => None,
            Err(sleep_until) => {
                let now = Utc::now();
                let time_between = sleep_until - now;

                let random_offset = Duration::milliseconds(
                    rand::distributions::Uniform::new(0, 10).sample(&mut thread_rng()),
                );
                let time_to_sleep = time_between + random_offset;

                Some(tokio::time::sleep(time_to_sleep.to_std().unwrap()))
            }
        }
    }

    /// Update the limit counters held in `self` from the headers in a given response.
    /// This method exists to correct any false optimistic updates set in the `check` method.
    #[inline]
    pub fn update(&self, url: &str, response: &Response) -> Result<(), Error> {
        let limit: u32 = response
            .headers()
            .get("X-RateLimit-Limit")
            .ok_or(Error::Other("missing X-RateLimit-Limit header"))?
            .to_str()
            .map_err(|_| Error::Other("non-string X-RateLimit-Limit header"))?
            .parse()
            .map_err(|_| Error::Other("non-number X-RateLimit-Limit header"))?;
        let remaining: u32 = response
            .headers()
            .get("X-RateLimit-Remaining")
            .ok_or(Error::Other("missing X-RateLimit-Remaining header"))?
            .to_str()
            .map_err(|_| Error::Other("non-string X-RateLimit-Remaining header"))?
            .parse()
            .map_err(|_| Error::Other("non-number X-RateLimit-Remaining header"))?;
        let reset: u64 = response
            .headers()
            .get("X-RateLimit-Reset")
            .ok_or(Error::Other("missing X-RateLimit-Reset header"))?
            .to_str()
            .map_err(|_| Error::Other("non-string X-RateLimit-Reset header"))?
            .parse()
            .map_err(|_| Error::Other("non-number X-RateLimit-Reset header"))?;

        let reset =
            Utc.from_utc_datetime(&NaiveDateTime::from_timestamp_opt(reset as i64, 0).unwrap());

        let is_global_limit: bool = response.headers().get("X-RateLimit-Global").is_some();
        if is_global_limit {
            let mut global = self.global.lock().expect("poisoned global counter");
            global.requests_made = global.limit;

            // close enough
            global.started_counting = reset;

            return Ok(());
        }

        let mut routes = self.routes.lock().expect("poisoned per-route counters");
        let route = routes
            .entry(url.to_string())
            .or_insert(LimitCounter::default());

        route.limit = limit;
        route.remaining = remaining as i32;
        route.window = reset;

        Ok(())
    }
}

/// A per query path rate limit counter.
/// This counter implements an inefficient reset timer due to needing to consume unix timestamps.
#[derive(Debug)]
struct LimitCounter {
    /// The absolute limit on how many requests can be sent.
    limit: u32,

    /// How many requests remain until the limit is reached.
    /// This value is affected by the value in the `window` field.
    remaining: i32,
    /// When the limit held in this counter will be cleared by discord.
    window: DateTime<Utc>,
}

impl LimitCounter {
    pub fn decrement_and_check(&mut self) -> Result<(), DateTime<Utc>> {
        self.remaining -= 1;

        if self.remaining > 0 {
            return Ok(());
        }

        let now = Utc::now();
        if now < self.window {
            // window is in the future

            Err(self.window.clone())
        } else {
            // window elapsed, and the limit is reset

            self.remaining = self.limit as i32;
            // this value is replaced once the discord servers respond with a real reset time
            // reset time is set to now plus one second in order to avoid race conditions, which lead to liberal limit application
            self.window = now + Duration::seconds(1);

            Ok(())
        }
    }
}

impl Default for LimitCounter {
    fn default() -> Self {
        Self {
            // The initial limit is assumed to be 5,
            // so that there is ample space for requests to fetch the real limit.
            limit: 5,

            remaining: 5,
            // The limit is said to have already expired because idk
            window: Utc::now(),
        }
    }
}

/// A counter for the global rate limit (50 requests per second).
///
/// This counter counts up until it hits 49 requests, at which time,
/// it checks if more than one second has elapsed since the last `started_counting` field update.
///
/// If more than one second has passed since the timer was set, the limit is assumed to have expired.
#[derive(Debug)]
struct GlobalLimitCounter {
    /// The upper limit on per second requests.
    limit: u32,

    /// How many requests were made since the `started_counting` timer was set.
    requests_made: u32,

    /// When the `remaining` field was first incremented from zero.
    ///
    /// This timer is often used as the timestamp after which **discord**
    /// counts the current limit against, even though that can't be the case.
    ///
    /// Now this is safe to do because the range of traffic in which there is enough to potentially hit the limit,
    /// but not enough to start counting close enough to the *real* **discord accounted** reset is very slim.
    ///
    /// If the above assumption is proven false, the penalty is a lot of small sleeps being executed.
    started_counting: DateTime<Utc>,
}

impl GlobalLimitCounter {
    /// Increment the limit counter, and then check if the limit will be reached by sending the request.
    ///
    /// `Ok` is returned of the limit has not been reached, and `Err` is returned if it has been.
    /// The value held in `Err` is when the limit is estimated to be reset.
    pub fn increment_and_check(&mut self) -> Result<(), DateTime<Utc>> {
        self.requests_made += 1;

        if self.requests_made < self.limit {
            return Ok(());
        }

        let mut start_plus_one = self.started_counting.clone();
        start_plus_one += Duration::seconds(1);

        let now = Utc::now();

        if start_plus_one > now {
            // start_plus_one is in the future

            Err(start_plus_one)
        } else {
            // start_plus_one elapsed, and the limit is reset

            self.requests_made = 0;
            self.started_counting = now;

            Ok(())
        }
    }
}

impl Default for GlobalLimitCounter {
    fn default() -> Self {
        Self {
            limit: 50,
            requests_made: 0,
            started_counting: Utc::now(),
        }
    }
}
