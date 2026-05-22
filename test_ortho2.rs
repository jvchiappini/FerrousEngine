fn main() {
    let mut matrix = glam::Mat4::from_scale_rotation_translation(
        glam::Vec3::ONE,
        glam::Quat::IDENTITY,
        glam::Vec3::new(200.0, 200.0, 0.0),
    );
    let wf = 1920.0;
    let hf = 1080.0;
    let ortho = glam::Mat4::orthographic_rh(0.0, wf, hf, 0.0, -1.0, 1.0);
    
    let opengl_matrix: glam::Mat4 = glam::Mat4::from_cols_array(&[
        1.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 0.5, 0.0,
        0.0, 0.0, 0.5, 1.0,
    ]);
    
    let mut screen_matrix = opengl_matrix * ortho * matrix;
    
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
    
    println!("Matrix Columns:");
    println!("col0: {:?}", screen_matrix.col(0));
    println!("col1: {:?}", screen_matrix.col(1));
    println!("col2: {:?}", screen_matrix.col(2));
    println!("col3: {:?}", screen_matrix.col(3));
    
    // simulate wgsl
    let mut shader_model = screen_matrix;
    let is_screen_space = shader_model.col(0).w > 0.5;
    if is_screen_space {
        let mut c0 = shader_model.col(0);
        c0.w = 0.0;
        shader_model = glam::Mat4::from_cols(c0, shader_model.col(1), shader_model.col(2), shader_model.col(3));
    }
    
    let hw = 250.0;
    let hh = 150.0;
    
    let pts = [
        glam::Vec4::new(-hw, -hh, -0.001, 1.0),
        glam::Vec4::new(hw, -hh, -0.001, 1.0),
        glam::Vec4::new(hw, hh, 0.001, 1.0),
        glam::Vec4::new(-hw, hh, 0.001, 1.0),
    ];
    
    for (i, p) in pts.iter().enumerate() {
        let out = shader_model * *p;
        println!("Point {}: {:?}", i, out);
    }
}
