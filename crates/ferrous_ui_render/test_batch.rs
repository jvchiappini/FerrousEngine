struct DrawSegment { quad_range: std::ops::Range<u32>, scissor: Option<()> }
struct GuiBatch { quads: Vec<u32>, segments: Vec<DrawSegment>, current_scissor: Option<()> }
impl GuiBatch {
    fn push_quad(&mut self, q: u32) {
        self.ensure_segment();
        self.quads.push(q);
        self.update_last_segment();
    }
    fn update_last_segment(&mut self) {
        self.ensure_segment();
        if let Some(last) = self.segments.last_mut() {
            last.quad_range.end = self.quads.len() as u32;
        }
    }
    fn ensure_segment(&mut self) {
        let needs_new = match self.segments.last() {
            Some(last) => last.scissor != self.current_scissor,
            None => true,
        };
        if needs_new {
            let q_start = self.quads.len() as u32;
            self.segments.push(DrawSegment { quad_range: q_start..q_start, scissor: self.current_scissor });
        }
    }
}
fn main() {
    let mut b = GuiBatch { quads: vec![], segments: vec![], current_scissor: None };
    b.push_quad(10); b.push_quad(20);
    println!("{:?}", (b.segments[0].quad_range.start, b.segments[0].quad_range.end));
}
