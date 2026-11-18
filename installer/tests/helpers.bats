setup() {
  TEST_ROOT="$(mktemp -d "${BATS_TMPDIR:-/tmp}/wobblescope-helpers.XXXXXX")"
  log() { :; }
  export -f log
  source "$BATS_TEST_DIRNAME/../installer.sh"
}
teardown() { rm -rf "$TEST_ROOT"; }

digest_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

@test "maps_aarch64_armv7l_and_x86_64" {
  run wobblescope_map_arch aarch64
  [ "$status" -eq 0 ]
  [ "$output" = "aarch64" ]
  run wobblescope_map_arch armv7l
  [ "$status" -eq 0 ]
  [ "$output" = "armv7" ]
  run wobblescope_map_arch x86_64
  [ "$status" -eq 0 ]
  [ "$output" = "x86_64" ]
}

@test "rejects_armv6l_and_unknown_arches_with_a_message" {
  for arch in armv6l riscv64 ""; do
    run wobblescope_map_arch "$arch"
    [ "$status" -eq 1 ]
    [ -n "$output" ]
  done
}

@test "checksum_passes_on_a_matching_line" {
  printf 'payload' > "$TEST_ROOT/wobblescope-armv7"
  printf '%s  wobblescope-armv7\n%s  wobblescope-x86_64\n' "$(digest_of "$TEST_ROOT/wobblescope-armv7")" \
    "$(printf 'x%.0s' {1..64})" > "$TEST_ROOT/SHA256SUMS"
  run wobblescope_verify_checksum "$TEST_ROOT/SHA256SUMS" wobblescope-armv7 "$TEST_ROOT/wobblescope-armv7"
  [ "$status" -eq 0 ]
}

@test "checksum_fails_on_a_mismatched_digest" {
  printf 'payload' > "$TEST_ROOT/wobblescope-armv7"
  printf '%s  wobblescope-armv7\n' "$(printf '0%.0s' {1..64})" > "$TEST_ROOT/SHA256SUMS"
  run wobblescope_verify_checksum "$TEST_ROOT/SHA256SUMS" wobblescope-armv7 "$TEST_ROOT/wobblescope-armv7"
  [ "$status" -ne 0 ]
}

@test "checksum_fails_when_the_asset_has_no_line" {
  printf 'payload' > "$TEST_ROOT/wobblescope-armv7"
  printf '%s  wobblescope-x86_64\n' "$(digest_of "$TEST_ROOT/wobblescope-armv7")" > "$TEST_ROOT/SHA256SUMS"
  run wobblescope_verify_checksum "$TEST_ROOT/SHA256SUMS" wobblescope-armv7 "$TEST_ROOT/wobblescope-armv7"
  [ "$status" -ne 0 ]
}

@test "checksum_fails_on_a_duplicated_line" {
  printf 'payload' > "$TEST_ROOT/wobblescope-armv7"
  d="$(digest_of "$TEST_ROOT/wobblescope-armv7")"
  printf '%s  wobblescope-armv7\n%s  wobblescope-armv7\n' "$d" "$d" > "$TEST_ROOT/SHA256SUMS"
  run wobblescope_verify_checksum "$TEST_ROOT/SHA256SUMS" wobblescope-armv7 "$TEST_ROOT/wobblescope-armv7"
  [ "$status" -ne 0 ]
}

@test "install_files_creates_binary_symlink_and_release_info_with_the_exact_tag" {
  printf 'binary' > "$TEST_ROOT/dl"
  run wobblescope_install_files "$TEST_ROOT/dest" wobblescope-armv7 "$TEST_ROOT/dl" v1.2.3-rc.1 mjonuschat wobblescope
  [ "$status" -eq 0 ]
  [ -x "$TEST_ROOT/dest/wobblescope-armv7" ]
  [ "$(cat "$TEST_ROOT/dest/wobblescope-armv7")" = "binary" ]
  [ "$(readlink "$TEST_ROOT/dest/wobblescope")" = "wobblescope-armv7" ]
  [ "$(cat "$TEST_ROOT/dest/wobblescope")" = "binary" ]
  grep -q '"project_name": *"wobblescope"' "$TEST_ROOT/dest/release_info.json"
  grep -q '"project_owner": *"mjonuschat"' "$TEST_ROOT/dest/release_info.json"
  grep -q '"version": *"v1.2.3-rc.1"' "$TEST_ROOT/dest/release_info.json"
  grep -q '"asset_name": *"wobblescope-armv7"' "$TEST_ROOT/dest/release_info.json"
}

@test "install_files_replaces_an_older_arch_binary_and_symlink" {
  printf 'old' > "$TEST_ROOT/dl-old"
  printf 'new' > "$TEST_ROOT/dl-new"
  wobblescope_install_files "$TEST_ROOT/dest" wobblescope-armv7 "$TEST_ROOT/dl-old" v1.0.0 mjonuschat wobblescope
  run wobblescope_install_files "$TEST_ROOT/dest" wobblescope-aarch64 "$TEST_ROOT/dl-new" v1.1.0 mjonuschat wobblescope
  [ "$status" -eq 0 ]
  [ "$(cat "$TEST_ROOT/dest/wobblescope")" = "new" ]
  [ -x "$TEST_ROOT/dest/wobblescope-aarch64" ]
  grep -q '"version": *"v1.1.0"' "$TEST_ROOT/dest/release_info.json"
  grep -q '"asset_name": *"wobblescope-aarch64"' "$TEST_ROOT/dest/release_info.json"
}
