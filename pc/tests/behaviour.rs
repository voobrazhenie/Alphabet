//! What the app does, checked without a GPU: the benchmark's determinism, the
//! camera, the march budget, and settings surviving a round trip through disk.

use cortical_flythrough::bench::{Bench, FRAMES, SHOTS};
use cortical_flythrough::camera::{self, Cam, Held, Rng};
use cortical_flythrough::gfx;
use cortical_flythrough::state::{Present, State, SCENE_R};
use std::time::{Duration, Instant};


/// The warm-up is 1.5 s of held pose, discarded. Two calls 800 ms apart take a run
/// past it without consuming any of the 600 measured frames.
fn past_warmup(b: &mut Bench, st: &mut State, cam: &mut Cam) -> Instant {
    let mut t = Instant::now();
    for _ in 0..2 {
        t += Duration::from_millis(800);
        b.step(t, 800.0, st, cam);
    }
    t
}

/// Frame N of a run has to show the same geometry on every machine at every frame
/// rate: the clocks step by a fixed 1/60 s per frame, never by elapsed time. Two
/// runs at wildly different speeds must produce identical poses and clocks.
#[test]
fn a_run_is_the_same_at_any_frame_rate() {
    let trace = |ms_per_frame: f32| {
        let mut st = State::default();
        let mut cam = Cam::default();
        let mut b = Bench::default();
        b.start(&st, 320.0);
        let mut out: Vec<(f32, [f32; 3], [f32; 3])> = Vec::new();
        let mut t = past_warmup(&mut b, &mut st, &mut cam);
        for _ in 0..600 {
            t += Duration::from_secs_f32(ms_per_frame / 1000.0);
            if b.step(t, ms_per_frame, &mut st, &mut cam) {
                break;
            }
            out.push((st.clock, cam.eye, cam.tgt));
        }
        out
    };
    let fast = trace(4.0); // 250 fps
    let slow = trace(28.0); // 36 fps
    assert_eq!(fast.len(), 600, "a full run is 600 measured frames");
    assert_eq!(fast.len(), slow.len(), "the frame count must not depend on the frame rate");
    for (i, (a, b)) in fast.iter().zip(slow.iter()).enumerate() {
        assert_eq!(a, b, "frame {i} differs between a fast and a slow machine");
    }
    // and the clock really is 1/60 per frame
    assert!((fast[1].0 - fast[0].0 - 1.0 / 60.0).abs() < 1e-6);
}

/// The four shots and their frame counts are the contract in REQUIREMENTS.md §11.
#[test]
fn the_run_covers_every_shot_and_reports_complete() {
    let mut st = State::default();
    let mut cam = Cam::default();
    let mut b = Bench::default();
    b.start(&st, 320.0);
    let mut t = past_warmup(&mut b, &mut st, &mut cam);
    let mut frames = 0;
    loop {
        t += Duration::from_millis(5);
        if b.step(t, 5.0, &mut st, &mut cam) {
            break;
        }
        frames += 1;
        assert!(frames <= SHOTS.len() * FRAMES, "the run overran its frame count");
    }
    assert_eq!(frames, SHOTS.len() * FRAMES);
    assert!(b.complete());
    // the clocks the run borrowed are handed back untouched
    assert_eq!(st.clock, State::default().clock);
    assert_eq!(st.m_clock, State::default().m_clock);
}

/// Every shot is scaled by the object's own radius, so the same run frames a brain
/// and a neuron the same way.
#[test]
fn shots_scale_with_the_object() {
    let mut cam = Cam::default();
    let mut b = Bench::default();
    let mut st = State::default();
    st.scene = 1; // neuron, radius 1.15
    b.start(&st, 320.0);
    let mut t = past_warmup(&mut b, &mut st, &mut cam);
    t += Duration::from_millis(5);
    b.step(t, 5.0, &mut st, &mut cam);
    // first frame of the "Pass" shot: (-1.65, 0.55, 0.10) x radius
    let r = SCENE_R[1];
    assert!((cam.eye[0] - -1.65 * r).abs() < 1e-4, "eye {:?}", cam.eye);
    assert!((cam.eye[1] - 0.55 * r).abs() < 1e-4);
    assert_eq!(cam.roll, 0.0, "roll is 0 throughout");
}

#[test]
fn aiming_points_the_camera_at_the_target() {
    let mut st = State::default();
    camera::aim_at(&mut st, [2.0, 1.0, -3.0], [0.1, -0.2, 0.3]);
    let f = camera::fly_fwd(&st);
    let want: [f32; 3] = [0.1 - 2.0, -0.2 - 1.0, 0.3 + 3.0];
    let l = (want[0] * want[0] + want[1] * want[1] + want[2] * want[2]).sqrt();
    for i in 0..3 {
        assert!((f[i] - want[i] / l).abs() < 1e-5, "axis {i}: {f:?}");
    }
}

/// A slot holds the whole camera, and recalling one restores the navigation mode it
/// was saved in.
#[test]
fn a_saved_view_comes_back_whole() {
    let mut st = State::default();
    let cam = Cam::default();
    let mut held = Held::default();
    camera::set_fly(&mut st, true, &cam, &mut held);
    st.fly_pos = [1.5, -0.25, 2.0];
    st.fly_yaw = 0.75;
    st.fly_pitch = -0.3;
    camera::fav_store(&mut st, 2);

    camera::set_fly(&mut st, false, &cam, &mut held);
    st.mode = 2;
    st.zoom = 1.4;
    assert!(camera::fav_recall(&mut st, 2, &cam, &mut held));
    assert!(st.fly, "the slot was stored in fly mode, so recalling it flies");
    assert_eq!(st.fly_pos, [1.5, -0.25, 2.0]);
    assert!((st.fly_yaw - 0.75).abs() < 1e-6);
    assert!(!camera::fav_recall(&mut st, 5, &cam, &mut held), "an empty slot recalls nothing");
}

/// `R` always frames the object rather than empty space.
#[test]
fn a_new_viewpoint_always_looks_at_the_object() {
    let mut rng = Rng::new(12345);
    for scene in 0..3 {
        let mut st = State::default();
        st.scene = scene;
        st.fly = true;
        let r = SCENE_R[scene];
        for _ in 0..200 {
            camera::random_view(&mut st, &Cam::default(), &mut rng);
            let f = camera::fly_fwd(&st);
            let p = st.fly_pos;
            // distance from the origin to the line of sight
            let dot = p[0] * f[0] + p[1] * f[1] + p[2] * f[2];
            let close = [p[0] - dot * f[0], p[1] - dot * f[1], p[2] - dot * f[2]];
            let miss = (close[0] * close[0] + close[1] * close[1] + close[2] * close[2]).sqrt();
            assert!(miss < 0.45 * r, "object {scene} is off screen: miss {miss}");
            let dist = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            assert!(dist < 1.5 * r, "object {scene}: camera {dist} away, too far to overrun the frame");
        }
    }
}

#[test]
fn the_march_budget_respects_the_pin_and_the_cap() {
    let mut st = State::default();
    st.steps = 320.0;
    st.steps_pin = true;
    assert_eq!(st.march_steps(768.0), 320.0, "pinned, the slider is the budget");
    assert_eq!(st.march_steps(96.0), 96.0, "a software renderer clamps it");

    st.steps_pin = false;
    st.warp_on = [true, false, false];
    st.warp_mode = 0;
    let auto = st.march_steps(768.0);
    assert!(auto > 320.0, "on auto a space warp asks for more steps, got {auto}");
    st.warp_mode = 1;
    assert!(
        st.march_steps(768.0) < auto,
        "the object warp is only paid near the surface, so it needs fewer"
    );
}

/// The two switches that gate a value rather than scale it.
#[test]
fn switches_reach_the_shader_as_gates() {
    let mut st = State::default();
    st.spike = 0.8;
    st.spike_on = false;
    st.lc_twirl = 1.9;
    st.lc_on[4] = false;
    let u = gfx::uniforms(&st, &Cam::default(), 100, 100, 320.0);
    assert_eq!(u.spike, 0.0, "impulses off means no travelling light anywhere");
    assert_eq!(u.lc_twirl, 0.0, "step 5 off means no twirl");
    assert_eq!(u.eps, 1.0 / st.eps, "the shader wants the reciprocal of the precision");

    st.spike_on = true;
    st.lc_on[4] = true;
    let u = gfx::uniforms(&st, &Cam::default(), 100, 100, 320.0);
    assert_eq!(u.spike, 0.8);
    assert_eq!(u.lc_twirl, 1.9);
}

/// Settings have to survive the round trip through the file, and an older file
/// missing a field must load rather than fail.
#[test]
fn settings_survive_a_round_trip() {
    let mut st = State::default();
    st.scene = 2;
    st.warp_amt = 0.31;
    st.favs[0] = Some(cortical_flythrough::state::Fav {
        fly: true,
        pos: [1.0, 2.0, 3.0],
        yaw: 0.5,
        pitch: -0.2,
        mode: 1,
        clock: 40.0,
        az: 0.1,
        el: 0.2,
        zoom: 0.9,
    });
    st.present = Present::Immediate;
    let text = serde_json::to_string(&st).unwrap();
    let back: State = serde_json::from_str(&text).unwrap();
    assert_eq!(back.scene, 2);
    assert!((back.warp_amt - 0.31).abs() < 1e-6);
    assert_eq!(back.favs[0].unwrap().pos, [1.0, 2.0, 3.0]);
    assert_eq!(back.present, Present::Immediate);

    // a file from an older build, or a hand-edited one
    let sparse: State = serde_json::from_str("{\"scene\":1}").unwrap();
    assert_eq!(sparse.scene, 1);
    assert_eq!(sparse.steps, State::default().steps, "missing fields fall back to the defaults");
}
