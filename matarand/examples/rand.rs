use matarand::Rand;

fn main() {
    let mut rng = Rand::new();

    // Generate a list of random integers
    let int_list: Vec<i64> = (0..10)
        .map(|_| rng.randint(-100, 100))
        .collect();

    // Generate a list of random floats
    let float_list: Vec<f64> = (0..10)
        .map(|_| rng.randfloat(0.0, 1.0))
        .collect();

    println!("Random Integers:");
    for (i, val) in int_list.iter().enumerate() {
        println!("  [{}] {}", i, val);
    }

    println!("\nRandom Floats:");
    for (i, val) in float_list.iter().enumerate() {
        println!("  [{}] {:.6}", i, val);
    }
}