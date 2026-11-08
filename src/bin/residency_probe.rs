//! Opt-in search ladder; the six other diagnostic operations remain unimplemented.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let argument = |name: &str| {
        args.windows(2)
            .find(|pair| pair[0] == name)
            .map(|p| p[1].as_str())
            .unwrap_or_else(|| panic!("missing {name}"))
    };
    assert_eq!(
        argument("--scenario"),
        "search_file_size_ceiling",
        "only search residency diagnostics are implemented"
    );
    let trial = scratchpad::profile::residency::search_trial(
        argument("--workload").parse().unwrap(),
        argument("--fixture"),
        argument("--rung"),
        argument("--launch").parse().unwrap(),
    );
    println!("{trial}");
}
