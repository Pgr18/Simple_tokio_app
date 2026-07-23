use com_port_plotter::data::rcm_pipeline::{RcmPipeline, RcmProfile};

fn main() {
    let mut p = RcmPipeline::new(RcmProfile::Rcms);
    let mut raw = [0u8; 20];
    for i in 0..10 {
        raw[i*2] = if i == 0 { 0x02 } else { 0x12 };
        raw[i*2+1] = 0x04;
    }
    let mut total_in = 0usize;
    let mut total_out = 0usize;
    for _ in 0..2000 {
        total_in += 1;
        total_out += p.process_raw(&raw).len();
    }
    println!("in={total_in} out={total_out} ratio={}", total_out as f64 / total_in as f64);
}
