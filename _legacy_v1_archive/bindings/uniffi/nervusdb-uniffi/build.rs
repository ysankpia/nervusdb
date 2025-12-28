fn main() {
    uniffi::generate_scaffolding("src/nervusdb.udl").expect("uniffi scaffolding generation failed");
}
