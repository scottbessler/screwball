const DOCKERFILE: &str = include_str!("../Dockerfile");

#[test]
fn docker_builder_copies_public_before_release_build() {
    let public_copy = line_number("COPY public ./public");
    let release_build = line_number("RUN cargo build --release");

    assert!(
        public_copy < release_build,
        "Docker builder stage must copy public assets before cargo build because \
         src/routes.rs embeds public/sw.js with include_str!",
    );
}

fn line_number(needle: &str) -> usize {
    DOCKERFILE
        .lines()
        .position(|line| line.trim() == needle)
        .unwrap_or_else(|| panic!("Dockerfile is missing `{needle}`"))
}
