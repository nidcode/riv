//! Synthetic catalog for the mock server. Fully generated from a fixed seed; no real AWS data.

use crate::api::{SeatAvailability, Session, SessionTime, Speaker};

/// Tiny deterministic PRNG (so the catalog is identical on every machine and run).
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[(self.next() as usize) % xs.len()]
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

const TOPICS: &[(&str, &str)] = &[
    ("Artificial Intelligence", "AIM"),
    ("Compute", "CMP"),
    ("Serverless", "SVS"),
    ("Security", "SEC"),
    ("Data Analytics", "ANT"),
    ("Containers", "CNS"),
    ("Databases", "DAT"),
    ("DevOps", "DOP"),
];
const SERVICES: &[&str] = &[
    "Amazon Bedrock", "AWS Lambda", "Amazon S3", "Amazon ECS", "Amazon EKS", "Amazon DynamoDB",
    "AWS IAM", "Amazon SageMaker AI", "Amazon Aurora", "AWS CDK", "Amazon Athena", "AWS Step Functions",
];
const VERBS: &[&str] = &["Building", "Scaling", "Securing", "Operating", "Debugging", "Optimizing", "Designing", "Migrating"];
const NOUNS: &[&str] = &[
    "agentic workflows", "event-driven systems", "multi-tenant platforms", "data pipelines", "retrieval pipelines",
    "serverless APIs", "zero-trust networks", "cost-aware architectures", "streaming ingestion", "platform teams",
];
const TYPES: &[&str] = &["Breakout session", "Chalk talk", "Workshop", "Builder session", "Lightning talk"];
const VENUES: &[&str] = &["Venetian", "Wynn", "MGM Grand", "Mandalay Bay"];
const SPEAKERS: &[&str] = &[
    "Alex Example", "Sam Sample", "Jordan Demo", "Casey Mock", "Riley Synthetic", "Morgan Test", "Taylor Fixture",
];
const DAYS: &[&str] = &["2026-11-30", "2026-12-01", "2026-12-02", "2026-12-03", "2026-12-04"];
const STARTS: &[&str] = &["08:30", "10:00", "11:30", "13:00", "14:30", "16:00"];

pub const EVENT_ID_PUBLIC: &str = "demo-public";
pub const EVENT_ID_REINVENT: &str = "demo-reinvent";
pub const TIMEZONE: &str = "America/Los_Angeles";

fn availability(r: &mut Lcg) -> (SeatAvailability, bool) {
    match r.below(10) {
        0..=4 => (SeatAvailability::Available, true),
        5..=6 => (SeatAvailability::Limited, true),
        7 => (SeatAvailability::VeryLimited, true),
        8 => (SeatAvailability::Unavailable, true),
        _ => (SeatAvailability::WalkUp, false),
    }
}

/// ~120 sessions ordered by sessionId, including `-R1/-R2` repeats of the same title.
pub fn generate() -> Vec<Session> {
    let mut r = Lcg(42);
    let mut out: Vec<Session> = Vec::new();
    let mut n = 0u32;

    let mut make = |r: &mut Lcg, title: Option<(String, String, String, String)>, suffix: &str, day: usize| -> Session {
        n += 1;
        let (topic, prefix) = *r.pick(TOPICS);
        let level = *r.pick(&[100u32, 200, 300, 300, 400]);
        let (title, abbr_base, service, topic) = title.unwrap_or_else(|| {
            let service = r.pick(SERVICES).to_string();
            (
                format!("{} {} with {}", r.pick(VERBS), r.pick(NOUNS), service),
                format!("{prefix}{level}"),
                service,
                topic.to_string(),
            )
        });
        let ty = *r.pick(TYPES);
        let len = if ty == "Workshop" { "120" } else if ty == "Lightning talk" { "20" } else { "60" };
        let (seat, reservable) = availability(r);
        let sp = r.pick(SPEAKERS).to_string();
        Session {
            session_id: format!("mock-{n:04}"),
            abstract_: Some(format!(
                "Synthetic abstract. Learn practical patterns for {} using {}. Level {level} audience; demo data only.",
                title.to_lowercase(),
                service
            )),
            title,
            abbreviation: Some(format!("{abbr_base}{suffix}")),
            type_: Some(ty.to_string()),
            level: Some(format!("{level} - {}", match level { 100 => "Foundational", 200 => "Intermediate", 300 => "Advanced", _ => "Expert" })),
            venue: Some(r.pick(VENUES).to_string()),
            room: Some(format!("Room {}", 100 + r.below(40))),
            is_all_day_session: Some(false),
            is_reservable: Some(reservable),
            seat_availability: Some(seat),
            session_time: Some(SessionTime {
                date: Some(DAYS[day].into()),
                time: Some(r.pick(STARTS).to_string()),
                length: Some(len.into()),
                timezone: Some(TIMEZONE.into()),
            }),
            speakers: vec![Speaker { name: Some(sp) }],
            topics: vec![topic],
            services: vec![service],
            tracks: vec![],
            ..Default::default()
        }
    };

    // 6 repeated titles (R1/R2 on different days)
    for k in 0..6 {
        let (topic, prefix) = TOPICS[k % TOPICS.len()];
        let service = SERVICES[(k * 2) % SERVICES.len()].to_string();
        let title = format!("Deep dive: {} for {}", VERBS[k % VERBS.len()], NOUNS[k % NOUNS.len()]);
        let base = format!("{prefix}30{}", k + 1);
        for (i, suffix) in ["-R1", "-R2"].iter().enumerate() {
            let day = (k + i * 2) % DAYS.len();
            out.push(make(&mut r, Some((title.clone(), base.clone(), service.clone(), topic.to_string())), suffix, day));
        }
    }
    while out.len() < 120 {
        let day = r.below(DAYS.len() as u64) as usize;
        out.push(make(&mut r, None, "", day));
    }
    out.sort_by(|a, b| a.session_id.cmp(&b.session_id));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_sized() {
        let a = generate();
        let b = generate();
        assert_eq!(a, b);
        assert_eq!(a.len(), 120);
        assert!(a.iter().any(|s| s.abbreviation.as_deref().is_some_and(|c| c.ends_with("-R1"))));
        let mut ids: Vec<_> = a.iter().map(|s| s.session_id.clone()).collect();
        ids.dedup();
        assert_eq!(ids.len(), 120);
    }
}
