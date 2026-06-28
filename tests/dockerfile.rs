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

#[test]
fn docker_builder_and_runtime_pin_the_same_debian_suite() {
    let build_image = build_image();
    let runtime_image = runtime_image();

    assert!(
        build_image.contains("bookworm"),
        "Rust builder image must pin the Debian suite so OpenSSL ABI does not drift",
    );
    assert!(
        runtime_image.contains("bookworm"),
        "Runtime image must stay on the same Debian suite as the Rust builder",
    );
}

fn line_number(needle: &str) -> usize {
    DOCKERFILE
        .lines()
        .position(|line| line.trim() == needle)
        .unwrap_or_else(|| panic!("Dockerfile is missing `{needle}`"))
}

fn build_image() -> &'static str {
    DOCKERFILE
        .lines()
        .find_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("FROM ") && trimmed.ends_with(" AS build") {
                Some(
                    trimmed
                        .trim_start_matches("FROM ")
                        .trim_end_matches(" AS build")
                        .trim(),
                )
            } else {
                None
            }
        })
        .unwrap_or_else(|| panic!("Dockerfile is missing a builder FROM line"))
}

fn runtime_image() -> &'static str {
    DOCKERFILE
        .lines()
        .find_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("FROM ") && !trimmed.contains(" AS ") {
                Some(trimmed.trim_start_matches("FROM ").trim())
            } else {
                None
            }
        })
        .unwrap_or_else(|| panic!("Dockerfile is missing a runtime FROM line"))
}
