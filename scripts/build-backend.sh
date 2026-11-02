#!/usr/bin/env bash
# Builds one whisper.cpp backend and packs it as a release artifact.
#
# Usage: build-backend.sh <cpu|vulkan|cuda|rocm> <version> <outdir>
# Run from inside the whisper.cpp checkout.

set -euo pipefail

backend="${1:?uso: build-backend.sh <backend> <version> <outdir>}"
version="${2:?falta la versión de whisper.cpp}"
out="${3:-dist}"

case "$backend" in
  cpu) flags=() ;;
  vulkan) flags=(-DGGML_VULKAN=ON) ;;
  cuda) flags=(-DGGML_CUDA=ON) ;;
  rocm)
    flags=(
      -DGGML_HIP=ON
      -DAMDGPU_TARGETS="gfx1100;gfx1101;gfx1102;gfx1200;gfx1201"
    )
    ;;
  *)
    echo "backend desconocido: $backend" >&2
    exit 2
    ;;
esac

echo "== whisper.cpp ${version} · backend ${backend} =="

# Extra CMake flags for cases that need them (CUDA VMM, custom toolchains).
extra=()
if [ -n "${EXTRA_CMAKE_ARGS:-}" ]; then
    # Word splitting is intended here.
    # shellcheck disable=SC2206
    extra=(${EXTRA_CMAKE_ARGS})
fi

cmake -B build \
  -DCMAKE_BUILD_TYPE=Release \
  -DBUILD_SHARED_LIBS=ON \
  -DWHISPER_BUILD_TESTS=OFF \
  -DWHISPER_BUILD_EXAMPLES=ON \
  -DCMAKE_INSTALL_RPATH='$ORIGIN' \
  -DCMAKE_BUILD_WITH_INSTALL_RPATH=ON \
  -DGGML_NATIVE=OFF \
  -DGGML_CPU_ALL_VARIANTS=ON \
  -DGGML_BACKEND_DL=ON \
  "${flags[@]}" \
  "${extra[@]}"

cmake --build build -j"$(nproc)"

stage="${out}/stage-${backend}"
rm -rf "$stage"
mkdir -p "$stage"

# Executables live in build/bin, but with BUILD_SHARED_LIBS=ON the libraries
# are written to build/src and build/ggml/src. Both must end up side by side
# or the binaries fail with "cannot open shared object file".
cp -a build/bin/. "$stage/"
find build \( -name '*.so' -o -name '*.so.*' \) -exec cp -a {} "$stage/" \;

# Strip what is safe to strip; a missing strip must not fail the build.
find "$stage" -type f \( -name 'whisper-*' -o -name '*.so*' \) -exec strip --strip-unneeded {} + 2>/dev/null || true

cp LICENSE "$stage/LICENSE.whisper.cpp"
cp README.md "$stage/README.whisper.cpp.md" 2>/dev/null || true

name="whisper-${version}-linux-x64-${backend}"
mkdir -p "$out"
tar -C "$stage" -czf "$out/${name}.tar.gz" .
rm -rf "$stage"

echo "listo: ${out}/${name}.tar.gz"
