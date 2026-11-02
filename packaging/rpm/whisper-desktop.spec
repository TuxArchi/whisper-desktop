# RPM spec for Whisper Desktop.
#
# Packages an already compiled binary: building Rust and Node inside rpmbuild
# would need network access and a 2 GB toolchain on the user's machine.
#
# Build (from the repository root):
#   scripts/make-rpm.sh <binary-dir> <version> <out-dir>

%global debug_package %{nil}
%global __strip /bin/true

Name:           whisper-desktop
Version:        %{version_override}
Release:        1%{?dist}
Summary:        Transcribe y traduce audio en local
License:        MIT
URL:            https://github.com/TuxArchi/whisper-desktop
BuildArch:      x86_64
Requires:       webkit2gtk4.1

%description
Whisper Desktop detects your GPU, picks the best whisper.cpp backend (CUDA,
Vulkan, ROCm or CPU) and the right model, then leaves the application ready to
transcribe and translate audio and video without Python and without sending
anything to the internet.

%prep

%build

%install
install -Dm755 %{_sourcedir}/whisper-desktop %{buildroot}%{_bindir}/whisper-desktop
install -Dm644 %{_sourcedir}/whisper-desktop.desktop \
    %{buildroot}%{_datadir}/applications/whisper-desktop.desktop
install -Dm644 %{_sourcedir}/whisper-desktop.png \
    %{buildroot}%{_datadir}/icons/hicolor/256x256/apps/whisper-desktop.png
install -Dm644 %{_sourcedir}/LICENSE \
    %{buildroot}%{_datadir}/licenses/whisper-desktop/LICENSE
install -Dm644 %{_sourcedir}/THIRD-PARTY.md \
    %{buildroot}%{_datadir}/licenses/whisper-desktop/THIRD-PARTY.md
%{_datadir}/licenses/whisper-desktop/NOTICES.md
%{_datadir}/licenses/whisper-desktop/LGPL-3.0-ffmpeg.txt
%{_datadir}/licenses/whisper-desktop/FFMPEG-SOURCE.txt
%{_datadir}/licenses/whisper-desktop/MIT-whisper.cpp.txt
install -Dm644 %{_sourcedir}/NOTICES.md \
    %{buildroot}%{_datadir}/licenses/whisper-desktop/NOTICES.md
for licence in %{_sourcedir}/licenses/*; do
    install -Dm644 "$licence" \
        %{buildroot}%{_datadir}/licenses/whisper-desktop/$(basename "$licence")
done

%files
%{_bindir}/whisper-desktop
%{_datadir}/applications/whisper-desktop.desktop
%{_datadir}/icons/hicolor/256x256/apps/whisper-desktop.png
%{_datadir}/licenses/whisper-desktop/LICENSE
%{_datadir}/licenses/whisper-desktop/THIRD-PARTY.md
%{_datadir}/licenses/whisper-desktop/NOTICES.md
%{_datadir}/licenses/whisper-desktop/LGPL-3.0-ffmpeg.txt
%{_datadir}/licenses/whisper-desktop/FFMPEG-SOURCE.txt
%{_datadir}/licenses/whisper-desktop/MIT-whisper.cpp.txt

%changelog
* Mon Nov 02 2026 TuxArchi <TuxArchi@users.noreply.github.com> - 0.1.0-1
- Primera versión pública.
