//! Parses one expression per dialect and prints what came out.
//!
//! Run with `cargo run --example parse`.

use cronp::{Quartz, Robfig, Schedule, Vixie};

fn main() {
  // Five fields. The annotated binding supplies the default year width, so the const
  // parameter never has to be written.
  let nightly: Schedule<Vixie> = match Schedule::parse("30 2 * * 1-5") {
    Ok(schedule) => schedule,
    Err(error) => {
      println!("could not parse: {error}");
      return;
    }
  };
  println!("Vixie   30 2 * * 1-5   -> {nightly:?}");

  // Six fields with a leading seconds field, `?` in one of the two day fields, and a
  // date predicate the other dialects do not have.
  match Schedule::<Quartz>::parse("0 15 10 L * ?") {
    Ok(schedule) => println!("Quartz  0 15 10 L * ?  -> {schedule:?}"),
    Err(error) => println!("could not parse: {error}"),
  }

  // A period rather than a set of instants.
  match Schedule::<Robfig>::parse("@every 90m") {
    Ok(schedule) => println!("Robfig  @every 90m     -> {:?}", schedule.every()),
    Err(error) => println!("could not parse: {error}"),
  }

  // The same expression under the wrong dialect, and what it says about it.
  match Schedule::<Quartz>::parse("30 2 * * 1-5") {
    Ok(_) => println!("unexpectedly accepted"),
    Err(error) => println!("Quartz  30 2 * * 1-5   -> {error}"),
  }

  // A legal Quartz year that the default width cannot hold, and the error that names
  // the width which can.
  match Schedule::<Quartz>::parse("0 0 0 ? * * 2098") {
    Ok(_) => println!("unexpectedly accepted"),
    Err(error) => println!("Quartz  ... 2098       -> {error}"),
  }
}
