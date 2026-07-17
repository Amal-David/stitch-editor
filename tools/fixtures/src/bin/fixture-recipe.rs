#![forbid(unsafe_code)]
#![deny(clippy::all, clippy::pedantic)]

use std::env;
use std::process::ExitCode;

use stitch_fixtures::{CorpusDuration, FixtureId, FixtureRecipe};

fn main() -> ExitCode {
    let mut arguments = env::args().skip(1);
    let fixture = arguments.next();
    let duration = arguments.next();
    if arguments.next().is_some() {
        return usage();
    }
    let Some(fixture) = fixture else {
        return usage();
    };
    let Some(duration) = duration else {
        return usage();
    };
    let Some(fixture_id) = parse_fixture(&fixture) else {
        return usage();
    };
    let Some(corpus_duration) = parse_duration(&duration) else {
        return usage();
    };
    match FixtureRecipe::new(fixture_id, corpus_duration).canonical_json() {
        Ok(recipe) => {
            println!("{recipe}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("failed to serialize fixture recipe: {error}");
            ExitCode::FAILURE
        }
    }
}

fn parse_fixture(value: &str) -> Option<FixtureId> {
    match value {
        "QHD-I" => Some(FixtureId::QhdI),
        "QHD-LGOP" => Some(FixtureId::QhdLgop),
        "3K-I" => Some(FixtureId::ThreeKI),
        "3K-LGOP" => Some(FixtureId::ThreeKLgop),
        "VFR-AV" => Some(FixtureId::VfrAv),
        _ => None,
    }
}

fn parse_duration(value: &str) -> Option<CorpusDuration> {
    match value {
        "development" => Some(CorpusDuration::Development),
        "release" => Some(CorpusDuration::Release),
        _ => None,
    }
}

fn usage() -> ExitCode {
    eprintln!("usage: fixture-recipe <QHD-I|QHD-LGOP|3K-I|3K-LGOP|VFR-AV> <development|release>");
    ExitCode::from(2)
}
