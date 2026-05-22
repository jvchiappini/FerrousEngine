fn main() {
    let wf = 1280.0;
    let hf = 720.0;
    let ortho = glam::Mat4::orthographic_rh(0.0, wf, hf, 0.0, -1.0, 1.0);
    
    let OPENGL_TO_WGPU_MATRIX: glam::Mat4 = glam::Mat4::from_cols(
        glam::Vec4::new(1.0, 0.0, 0.0, 0.0),
        glam::Vec4::new(0.0, 1.0, 0.0, 0.0),
        glam::Vec4::new(0.0, 0.0, 0.5, 0.5),
        glam::Vec4::new(0.0, 0.0, 0.0, 1.0),
    );
    
    let mut matrix = glam::Mat4::from_translation(glam::Vec3::new(200.0, 200.0, 0.0));
    let mut screen_matrix = OPENGL_TO_WGPU_MATRIX * ortho * matrix;
    
    let mut col0 = screen_matrix.col(0);
    let mut col1 = screen_matrix.col(1);
    let mut col2 = screen_matrix.col(2);
    let mut col3 = screen_matrix.col(3);
    col0.z = 0.0;
    col1.z = 0.0;
    col2.z = 0.0;
    col3.z = 0.01;
    col0.w = 1.0;
    
    screen_matrix = glam::Mat4::from_cols(col0, col1, col2, col3);
    
    // Shader simulation
    let is_screen = screen_matrix.col(0).w > 0.5;
    if is_screen {
        let mut c0 = screen_matrix.col(0);
        c0.w = 0.0;
        screen_matrix = glam::Mat4::from_cols(c0, screen_matrix.col(1), screen_matrix.col(2), screen_matrix.col(3));
    }
    
    let p1 = glam::Vec4::new(-250.0, -150.0, 0.0, 1.0);
    let out1 = screen_matrix * p1;
    let p2 = glam::Vec4::new(250.0, 150.0, 0.0, 1.0);
    let out2 = screen_matrix * p2;
    
    println!("Point 1 (-250, -150): {:?}", out1);
    println!("Point 2 (250, 150): {:?}", out2);
}
