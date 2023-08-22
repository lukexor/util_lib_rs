use util_lib_rs::profile;

#[test]
fn my_function() {
    profile!();

    for _ in 0..10000 {
        profile!("loop");
    }
}
