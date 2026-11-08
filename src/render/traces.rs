//! Where every body has been. Positions are sampled from the snapshots the app reads anyway, so
//! this is display history only and never feeds back into the simulation.
//!
//! A trace can be drawn two ways:
//!
//! - Absolute: the path through space, in the simulation's own frame. The Moon's is a wavy line
//!   along the Earth's orbit.
//! - Relative: the path around the body it orbits, which moves along with it. The Moon's is a loop
//!   around the Earth. Bodies that orbit nothing only have the absolute one.
//!
//! Each sample remembers what the body was orbiting and where that was, so both come from the
//! same samples. A sample is added whenever the body's direction of travel has turned another
//! degree in either frame. Samples also keep the velocity, so the trace is drawn as a smooth curve
//! through them (see [`super::curves`]), even when the simulation runs so fast that a body turns
//! tens of degrees between snapshots.
//!
//! How long a trace lasts goes by real time, not by simulation time or distance: it stays bright
//! for [`HOLD_SECONDS`] and then fades out over [`FADE_SECONDS`], at any speed. The clock only runs
//! while the simulation does, so pausing freezes the traces as they are.

use std::collections::HashMap;
use std::collections::VecDeque;

use glam::DVec3;

use super::curves::Point;
use solar_rust::astronomy::{Bodies, BodyId};

/// A sample is added once the direction of travel has turned this far (degrees)
const STEP_DEGREES: f64 = 1.0;
/// If a body goes further around its parent than this between two snapshots, the curve between
/// them would no longer follow the orbit (the simulation runs too fast to follow it), so its
/// relative trace starts over. The absolute trace always carries on.
const MAX_RELATIVE_STEP_DEGREES: f64 = 60.0;
/// At most this many samples per body, in case a body turns very fast for a long time
const MAX_SAMPLES: usize = 4096;
/// A trace stays fully bright this long (seconds of the simulation running)...
pub const HOLD_SECONDS: f64 = 3.0;
/// ...and then fades out over this long
pub const FADE_SECONDS: f64 = 2.0;
/// After this, a trace is gone
pub const LIFETIME_SECONDS: f64 = HOLD_SECONDS + FADE_SECONDS;
/// The clock advances by real time between snapshots, but at most this much at once, so a stall
/// or a pause doesn't age everything in one jump
const MAX_CLOCK_STEP: f64 = 0.1;

/// How bright a trace is `age` seconds after the body was there: 1 at first, then smoothly down
/// to 0
pub fn brightness(age: f64) -> f64 {
    let t = ((age - HOLD_SECONDS) / FADE_SECONDS).clamp(0.0, 1.0);
    1.0 - t * t * (3.0 - 2.0 * t)
}

#[derive(Default)]
pub struct Traces {
    traces: HashMap<BodyId, Trace>,
    last_time: Option<f64>,
    /// Seconds the simulation has run while being traced, and the real time it was last advanced
    clock: f64,
    last_real: Option<f64>,
}

/// A point on a trace: where the body was, and how many seconds ago (by the trace clock)
#[derive(Clone, Copy, Debug)]
pub struct TracePoint {
    pub point: Point,
    pub age: f64,
}

/// Where a body was, how it moved, and what it orbited (id, position, velocity), at a simulation
/// time and a trace clock time
#[derive(Clone, Copy)]
struct State {
    position: DVec3,
    velocity: DVec3,
    time: f64,
    clock: f64,
    parent: Option<(BodyId, DVec3, DVec3)>,
}

impl State {
    fn absolute(&self) -> Point {
        Point {
            position: self.position,
            velocity: self.velocity,
            time: self.time,
        }
    }

    /// Relative to its parent at the time
    fn relative(&self) -> Point {
        let (position, velocity) = self
            .parent
            .map_or((DVec3::ZERO, DVec3::ZERO), |(_, p, v)| (p, v));
        Point {
            position: self.position - position,
            velocity: self.velocity - velocity,
            time: self.time,
        }
    }

    fn parent_id(&self) -> Option<BodyId> {
        self.parent.map(|(id, ..)| id)
    }
}

struct Sample {
    state: State,
    /// Whether the relative trace starts here: the parent changed, or the body moved too fast
    /// around it to follow
    relative_start: bool,
}

/// A direction of travel relative to a parent
type Relative = Option<(BodyId, DVec3)>;

pub struct Trace {
    samples: VecDeque<Sample>,
    /// The latest snapshot, where the trace ends
    latest: Option<State>,
    /// Direction of travel relative to the parent in the last snapshot
    relative: Relative,
    /// Directions of travel at the last sample
    heading_at_sample: DVec3,
    relative_at_sample: Relative,
}

impl Trace {
    fn new(heading: DVec3) -> Self {
        Trace {
            samples: VecDeque::new(),
            latest: None,
            relative: None,
            heading_at_sample: heading,
            relative_at_sample: None,
        }
    }

    /// Takes in a snapshot
    fn observe(&mut self, state: State) {
        self.latest = Some(state);
        self.forget(state.clock);
        let Some(heading) = state.velocity.try_normalize() else {
            return;
        };

        let relative = state.parent.and_then(|(id, _, parent_velocity)| {
            Some((id, (state.velocity - parent_velocity).try_normalize()?))
        });
        // The relative direction moved smoothly since the last snapshot
        let followed = match (relative, self.relative) {
            (Some((id, now)), Some((was_id, was))) => {
                id == was_id && was.angle_between(now).to_degrees() <= MAX_RELATIVE_STEP_DEGREES
            }
            _ => false,
        };
        self.relative = relative;
        if !followed {
            self.relative_at_sample = None;
        }

        let turn = self.heading_at_sample.angle_between(heading).to_degrees();
        let relative_turn = match (relative, self.relative_at_sample) {
            (Some((id, now)), Some((was_id, was))) if id == was_id => {
                Some(was.angle_between(now).to_degrees())
            }
            _ => None,
        };
        // A relative trace can begin once the body is followed smoothly around its parent
        let begins = followed && relative_turn.is_none();
        let turned = turn >= STEP_DEGREES || relative_turn.is_some_and(|t| t >= STEP_DEGREES);
        if self.samples.is_empty() || turned || begins {
            self.samples.push_back(Sample {
                state,
                relative_start: relative_turn.is_none(),
            });
            self.heading_at_sample = heading;
            self.relative_at_sample = if followed { relative } else { None };
        }
    }

    /// Drops samples that have faded out, keeping the one the visible part starts from
    fn forget(&mut self, clock: f64) {
        while self.samples.len() > MAX_SAMPLES
            || self
                .samples
                .get(1)
                .is_some_and(|next| clock - next.state.clock >= LIFETIME_SECONDS)
        {
            self.samples.pop_front();
        }
    }

    /// The absolute trace, oldest first, ending where the body is now. The oldest point may be
    /// older than [`LIFETIME_SECONDS`]: the trace fades out between it and the next.
    pub fn absolute(&self) -> Vec<TracePoint> {
        self.walk_back(|_| true, State::absolute, |_| false)
    }

    /// The trace around `parent`, relative to it, oldest first, ending where the body is now.
    /// Empty if the body isn't being followed around it.
    pub fn relative(&self, parent: BodyId) -> Vec<TracePoint> {
        let following = self.relative_at_sample.is_some_and(|(id, _)| id == parent)
            && self.latest.is_some_and(|s| s.parent_id() == Some(parent));
        if !following {
            return Vec::new();
        }
        self.walk_back(
            |s| s.state.parent_id() == Some(parent),
            State::relative,
            |s| s.relative_start,
        )
    }

    /// Goes back from where the body is now while `include` holds, until a sample where `last`
    /// holds or the first one that has faded out
    fn walk_back(
        &self,
        include: impl Fn(&Sample) -> bool,
        point: impl Fn(&State) -> Point,
        last: impl Fn(&Sample) -> bool,
    ) -> Vec<TracePoint> {
        let Some(latest) = self.latest else {
            return Vec::new();
        };
        let at = |state: &State| TracePoint {
            point: point(state),
            age: latest.clock - state.clock,
        };
        let mut points = Vec::new();
        if self
            .samples
            .back()
            .is_none_or(|s| s.state.time != latest.time)
        {
            points.push(at(&latest));
        }
        for sample in self.samples.iter().rev() {
            if !include(sample) {
                break;
            }
            let point = at(&sample.state);
            points.push(point);
            if last(sample) || point.age >= LIFETIME_SECONDS {
                break;
            }
        }
        points.reverse();
        points
    }
}

impl Traces {
    /// Samples the bodies at simulation time `time` (s), with `real` the real time (s, from any
    /// start). `parents` are what they orbit, from [`Bodies::orbital_parents`].
    pub fn update(&mut self, bodies: &Bodies, parents: &[Option<usize>], time: f64, real: f64) {
        if self.last_time == Some(time) {
            return;
        }
        // Time went backwards: the history no longer applies
        if self.last_time.is_some_and(|last| time < last) {
            self.traces.clear();
        }
        self.last_time = Some(time);
        if let Some(last_real) = self.last_real {
            self.clock += (real - last_real).clamp(0.0, MAX_CLOCK_STEP);
        }
        self.last_real = Some(real);
        self.traces.retain(|id, _| bodies.index_of(*id).is_some());

        for (i, parent) in parents.iter().enumerate() {
            let parent =
                parent.map(|p| (bodies.info[p].id, bodies.position[p], bodies.velocity[p]));
            let velocity = bodies.velocity[i];
            self.traces
                .entry(bodies.info[i].id)
                .or_insert_with(|| Trace::new(velocity.normalize_or(DVec3::X)))
                .observe(State {
                    position: bodies.position[i],
                    velocity,
                    time,
                    clock: self.clock,
                    parent,
                });
        }
    }

    pub fn get(&self, id: BodyId) -> Option<&Trace> {
        self.traces.get(&id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DQuat;
    use solar_rust::astronomy::AU;
    use solar_rust::ephemeris::solar_system;

    /// Snapshots per real second
    const FPS: f64 = 60.0;

    fn index(bodies: &Bodies, name: &str) -> usize {
        bodies.info.iter().position(|b| &*b.name == name).unwrap()
    }

    /// Moves the Earth around a circle, `step` degrees per snapshot, from snapshot `from`
    fn circle(bodies: &mut Bodies, traces: &mut Traces, from: usize, steps: usize, step: f64) {
        let earth = index(bodies, "Earth");
        let (start, speed) = (bodies.position[earth], bodies.velocity[earth].length());
        for k in from..from + steps {
            let turn = DQuat::from_rotation_z((k as f64 * step).to_radians());
            bodies.position[earth] = turn * start;
            bodies.velocity[earth] = turn * DVec3::Z.cross(start).normalize() * speed;
            traces.update(bodies, &vec![None; bodies.len()], k as f64, k as f64 / FPS);
        }
    }

    #[test]
    fn fades_out() {
        assert_eq!(brightness(0.0), 1.0);
        assert_eq!(brightness(HOLD_SECONDS), 1.0);
        assert!((brightness(HOLD_SECONDS + FADE_SECONDS / 2.0) - 0.5).abs() < 1e-12);
        assert_eq!(brightness(LIFETIME_SECONDS), 0.0);
        let mut previous = 1.0;
        for k in 0..100 {
            let b = brightness(k as f64 * 0.1);
            assert!(b <= previous);
            previous = b;
        }
    }

    #[test]
    fn a_point_every_degree_for_its_lifetime() {
        let mut bodies = solar_system();
        let earth = bodies.info[index(&bodies, "Earth")].id;
        let mut traces = Traces::default();

        // 0.55 degrees per snapshot: a point every other snapshot
        circle(&mut bodies, &mut traces, 0, 21, 0.55);
        assert_eq!(traces.get(earth).unwrap().absolute().len(), 11);

        // Going around for 30 seconds keeps the last 5, and one point before them
        circle(&mut bodies, &mut traces, 21, 1800, 0.55);
        let trace = traces.get(earth).unwrap().absolute();
        assert!(trace[0].age >= LIFETIME_SECONDS);
        assert!(trace[1].age < LIFETIME_SECONDS);
        let expected = (LIFETIME_SECONDS * FPS / 2.0) as usize;
        assert!(trace.len().abs_diff(expected) <= 2, "{}", trace.len());
        assert_eq!(trace.last().unwrap().age, 0.0);
    }

    /// Paused, nothing ages
    #[test]
    fn pausing_freezes_traces() {
        let mut bodies = solar_system();
        let earth = bodies.info[index(&bodies, "Earth")].id;
        let mut traces = Traces::default();
        circle(&mut bodies, &mut traces, 0, 60, 0.55);
        let none = vec![None; bodies.len()];
        // Ten seconds of the same snapshot
        for k in 0..600 {
            traces.update(&bodies, &none, 59.0, 1.0 + k as f64 / FPS);
        }
        let oldest = traces.get(earth).unwrap().absolute()[0].age;
        assert!((oldest - 59.0 / FPS).abs() < 1e-9, "{oldest}");
    }

    /// Going straight adds no points: the trace is a line from where it started to where it is
    #[test]
    fn straight_lines_need_no_points() {
        let mut bodies = solar_system();
        let mut traces = Traces::default();
        let voyager = index(&bodies, "Voyager 1");
        let none = vec![None; bodies.len()];
        for k in 0..100 {
            bodies.position[voyager] += bodies.velocity[voyager] * 86400.0;
            traces.update(&bodies, &none, k as f64, k as f64 / FPS);
        }
        let trace = traces.get(bodies.info[voyager].id).unwrap().absolute();
        assert_eq!(trace.len(), 2);
    }

    /// The Moon going around the Earth while the Earth goes around the Sun: around the Earth it's
    /// a circle, through space a gently wavy line
    #[test]
    fn relative_and_absolute() {
        let mut bodies = solar_system();
        let (sun, earth, moon) = (
            index(&bodies, "Sun"),
            index(&bodies, "Earth"),
            index(&bodies, "Moon"),
        );
        let mut traces = Traces::default();
        let (lunar_distance, lunar_speed) = (3.844E8, 1022.0);
        let earth_speed = 29_780.0;
        let day = 86400.0;
        for k in 0..400 {
            // A tenth of a day per snapshot, for 40 days, in 4 seconds
            let t = k as f64 * 0.1 * day;
            let earth_angle = earth_speed / AU * t;
            let moon_angle = lunar_speed / lunar_distance * t;
            let around =
                |angle: f64, radius: f64| DVec3::new(angle.cos(), angle.sin(), 0.0) * radius;
            let along = |angle: f64, speed: f64| DVec3::new(-angle.sin(), angle.cos(), 0.0) * speed;
            bodies.position[sun] = DVec3::ZERO;
            bodies.velocity[sun] = DVec3::ZERO;
            bodies.position[earth] = around(earth_angle, AU);
            bodies.velocity[earth] = along(earth_angle, earth_speed);
            bodies.position[moon] = bodies.position[earth] + around(moon_angle, lunar_distance);
            bodies.velocity[moon] = bodies.velocity[earth] + along(moon_angle, lunar_speed);
            let parents = bodies.orbital_parents();
            assert_eq!(parents[moon], Some(earth));
            traces.update(&bodies, &parents, t, k as f64 / 100.0);
        }

        let trace = traces.get(bodies.info[moon].id).unwrap();
        let relative = trace.relative(bodies.info[earth].id);
        // All 40 days of it: 1.46 times around
        let swept: f64 = relative
            .windows(2)
            .map(|pair| {
                let (a, b) = (pair[0].point.position, pair[1].point.position);
                a.angle_between(b).to_degrees()
            })
            .sum();
        let expected = (lunar_speed / lunar_distance * 39.8 * day).to_degrees();
        assert!((swept - expected).abs() < 2.0, "{swept} {expected}");
        for point in &relative {
            assert!((point.point.position.length() / lunar_distance - 1.0).abs() < 1e-6);
        }
        // The same samples drawn through space
        let absolute = trace.absolute();
        assert!(absolute.len() >= relative.len() && relative.len() > 390);
        // Around something else, it has no relative trace
        assert!(trace.relative(bodies.info[sun].id).is_empty());
    }

    /// Adrastea around Jupiter, `days` per snapshot at 60 snapshots a second, for ten days:
    /// returns its trace and the seconds since the start after every snapshot
    fn adrastea(days: f64, mut check: impl FnMut(&Trace, f64)) {
        let mut bodies = solar_system();
        let (jupiter, adrastea) = (index(&bodies, "Jupiter"), index(&bodies, "Adrastea"));
        let mut traces = Traces::default();
        let (radius, speed) = (1.29E8, 31_400.0);
        let start = bodies.position[jupiter];
        let steps = (10.0 / days) as usize;
        for k in 0..steps {
            let t = k as f64 * days * 86400.0;
            let angle = speed / radius * t;
            bodies.position[jupiter] = start + bodies.velocity[jupiter] * t;
            bodies.position[adrastea] =
                bodies.position[jupiter] + DVec3::new(angle.cos(), angle.sin(), 0.0) * radius;
            bodies.velocity[adrastea] =
                bodies.velocity[jupiter] + DVec3::new(-angle.sin(), angle.cos(), 0.0) * speed;
            let mut parents = vec![None; bodies.len()];
            parents[adrastea] = Some(jupiter);
            let real = k as f64 / FPS;
            traces.update(&bodies, &parents, t, real);
            check(traces.get(bodies.info[adrastea].id).unwrap(), real);
        }
    }

    /// Whether `trace` reaches back as far as it should, `elapsed` seconds in
    fn reaches_back(trace: &[TracePoint], elapsed: f64) -> bool {
        trace[0].age >= elapsed.min(LIFETIME_SECONDS) - 1e-9
    }

    /// At a day a second, Adrastea goes 20 degrees around Jupiter per snapshot, and through space
    /// it turns even faster when it swings backwards. Its traces still never start over.
    #[test]
    fn fast_moon_keeps_its_trace() {
        let id = solar_system().info[index(&solar_system(), "Jupiter")].id;
        adrastea(1.0 / 60.0, |trace, elapsed| {
            assert!(reaches_back(&trace.absolute(), elapsed));
            // The relative trace begins with the second snapshot
            if elapsed > 0.0 {
                let relative = trace.relative(id);
                assert!(reaches_back(&relative, elapsed - 1.0 / FPS));
            }
        });
    }

    /// At ten days a second it goes a third of the way around per snapshot: too fast to draw
    /// around Jupiter, but its absolute trace carries on
    #[test]
    fn too_fast_to_follow_around_its_parent() {
        let id = solar_system().info[index(&solar_system(), "Jupiter")].id;
        adrastea(1.0 / 6.0, |trace, elapsed| {
            assert!(trace.relative(id).is_empty());
            assert!(reaches_back(&trace.absolute(), elapsed));
        });
    }
}
