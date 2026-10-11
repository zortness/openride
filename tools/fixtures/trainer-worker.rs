//! Hardware-free process fixture for the Godot trainer integration test.
use std::io;
fn emit(row: &str) { println!("{row}"); }
fn state(phase: &str, epoch: u64) {
    println!(r#"{{"event":"session_state","state":"{phase}","resume_epoch":{epoch},"can_resume":{}}}"#, phase == "paused");
}
fn main() {
    let path = std::env::args().find_map(|a| a.strip_prefix("--live-session=").map(str::to_owned)).unwrap();
    let route=std::env::args().find_map(|a|a.strip_prefix("--route-profile=").map(str::to_owned));
    if let Some(ref route)=route {assert!(std::path::Path::new(route).is_file());}
    if route.is_some() { assert!(std::env::args().any(|a|a.starts_with("--ant-hr-config="))); }
    emit(r#"{"event":"session_check_created"}"#);
    emit(r#"{"event":"command_acknowledged","opcode":7}"#);
    state(if route.is_some(){"paused"}else{"running"}, if route.is_some(){1}else{0});
    let mut epoch = if route.is_some(){1}else{0};
    let mut paused = route.is_some();
    let mut history = String::new();
    loop {
        let mut command = String::new();
        let count = io::stdin().read_line(&mut command).unwrap();
        if count == 0 { history.push_str("eof\n"); break; }
        history.push_str(&command);
        match command.trim() {
            "pause" => {
                emit(r#"{"event":"command_acknowledged","opcode":17,"bytes":[17,0,0,0,0,40,51]}"#);
                emit(r#"{"event":"command_acknowledged","opcode":8}"#);
                epoch += 1;
                paused = true;
                emit(r#"{"event":"ant_hr_status","status":"unavailable"}"#);
                state("paused", epoch);
            }
            "finish" => break,
            resume => {
                assert_eq!(resume, format!("resume {epoch}"));
                assert!(paused);
                paused = false;
                emit(r#"{"event":"command_acknowledged","opcode":7}"#);
                emit(r#"{"event":"indoor_bike_data","decoded":{"Ok":{"cadence_rpm":82}},"elapsed_ms":1000,"received_ms":1000}"#);
                emit(r#"{"event":"ant_hr_data","heart_rate_bpm":88,"elapsed_ms":1000,"received_ms":1000}"#);
                state("running", epoch);
                if route.is_some() {emit(r#"{"event":"virtual_state","distance_m":10,"speed_kph":12,"course_grade_percent":1.7,"acknowledged_grade_percent":1,"power_w":200,"simulation_state":"running"}"#);}
            }
        }
    }
    std::fs::write(std::path::Path::new(&path).with_file_name("fixture-control.txt"), history).unwrap();
    emit(r#"{"event":"command_acknowledged","opcode":17,"bytes":[17,0,0,0,0,40,51]}"#);
    emit(r#"{"event":"command_acknowledged","opcode":8}"#);
    state("finished", epoch);
    emit(r#"{"event":"session_check_complete","reason":"user_finish","pause_acknowledged":true}"#);
    emit(r#"{"event":"device_closed","success":true}"#);
}
