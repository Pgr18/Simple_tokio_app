fn main() {
    use com_port_plotter::data::bin_playback::{decode_bin_bytes, guess_profile_from_path};
    use std::path::PathBuf;
    let path = PathBuf::from("tests/fixtures/2026-07-23 10-21-13 574 rcms.bin");
    let bytes = std::fs::read(&path).unwrap();
    let r = decode_bin_bytes(&bytes, path, guess_profile_from_path(PathBuf::from("x rcms.bin").as_path())).unwrap();
    println!("frames={} out={} dur={:.1}s", r.stats.frames, r.stats.samples_out, r.stats.duration_s);
    let mid = r.points.len()/2;
    let t0 = r.points[mid].time_seconds;
    let win: Vec<_> = r.points.iter().filter(|p| p.time_seconds>=t0 && p.time_seconds<t0+2.0).collect();
    let mut rmin=f64::INFINITY; let mut rmax=f64::NEG_INFINITY;
    let mut emin=f64::INFINITY; let mut emax=f64::NEG_INFINITY;
    for p in &win {
        rmin=rmin.min(p.rheo1); rmax=rmax.max(p.rheo1);
        emin=emin.min(p.ecg); emax=emax.max(p.ecg);
    }
    println!("2s filtered @mid n={}: rheo1[{:.1},{:.1}] ecg[{:.1},{:.1}]", win.len(), rmin, rmax, emin, emax);
    for p in win.iter().step_by(40).take(6) {
        println!("  t={:.3} r1={:.1} b1={:.1} ecg={:.1} r2={:.1}", p.time_seconds, p.rheo1, p.base1, p.ecg, p.rheo2);
    }
}
