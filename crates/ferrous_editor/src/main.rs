use ferrous_core::Transform;

fn main() {
    println!("Ferrous editor starting...");
    let t = Transform::default();
    println!("Default transform: {:?}", t.position);
}
