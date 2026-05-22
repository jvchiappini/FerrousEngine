fn main() {
    let wf = 1920.0;
    let hf = 1080.0;
    
    // glam is already a dependency of ferrous_renderer
    let open_gl_to_wgpu_matrix = glam::Mat4::from_cols(
        glam::Vec4::new(1.0, 0.0, 0.0, 0.0),
        glam::Vec4::new(0.0, 1.0, 0.0, 0.0),
        glam::Vec4::new(0.0, 0.0, 0.5, 0.0),
        glam::Vec4::new(0.0, 0.0, 0.5, 1.0),
    );

    let ortho = glam::Mat4::orthographic_rh(0.0, wf, hf, 0.0, -1.0, 1.0);
    // Let's create the translation matrix exactly like frame_builder.rs
    let matrix = glam::Mat4::from_translation(glam::Vec3::new(200.0, 200.0, 0.0));

    let mut screen_matrix = open_gl_to_wgpu_matrix * ortho * matrix;

    let mut col0 = screen_matrix.col(0);
    let mut col1 = screen_matrix.col(1);
    let mut col2 = screen_matrix.col(2);
    let mut col3 = screen_matrix.col(3);
    col0.z = 0.0;
    col1.z = 0.0;
    col2.z = 0.0;
    col3.z = 0.01;
    col0.w = 1.0;

    let model = glam::Mat4::from_cols(col0, col1, col2, col3);

    // Simulate WGSL shader behavior
    let mut shader_model = model;
    let is_screen_space = shader_model.col(0).w.abs() > 0.5;
    println!("Is screen space? {}", is_screen_space);
    if is_screen_space {
        let mut c0 = shader_model.col(0);
        c0.w = 0.0;
        shader_model = glam::Mat4::from_cols(c0, shader_model.col(1), shader_model.col(2), shader_model.col(3));
    }

    let pos = glam::Vec4::new(-250.0, -150.0, 0.0, 1.0); // Top-left vertex
    
    println!("ortho:");
    println!("{:?}", ortho.col(0));
    println!("{:?}", ortho.col(1));
    println!("{:?}", ortho.col(2));
    println!("{:?}", ortho.col(3));
}
