# Homebrew formula for perch.
#
# To consume from a tap repo (recommended):
#
#   brew tap luowensheng/perch https://github.com/luowensheng/homebrew-perch
#   brew install perch
#
# Until that tap exists, install directly:
#
#   brew install https://raw.githubusercontent.com/luowensheng/perch/main/Formula/perch.rb
#
# Sha256 placeholders below need to be filled in by the release workflow
# (or the tap repo's update script). Until then, install via go or the
# install.sh script.

class Perch < Formula
  desc     "Cross-platform command runner. One .perch file → CLI, web UI, REPL, or a portable binary."
  homepage "https://github.com/luowensheng/perch"
  version  "0.1.0"
  license  "Apache-2.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/luowensheng/perch/releases/download/v#{version}/perch-darwin-arm64"
      sha256 "REPLACE_ME_DARWIN_ARM64"
    else
      url "https://github.com/luowensheng/perch/releases/download/v#{version}/perch-darwin-amd64"
      sha256 "REPLACE_ME_DARWIN_AMD64"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/luowensheng/perch/releases/download/v#{version}/perch-linux-arm64"
      sha256 "REPLACE_ME_LINUX_ARM64"
    else
      url "https://github.com/luowensheng/perch/releases/download/v#{version}/perch-linux-amd64"
      sha256 "REPLACE_ME_LINUX_AMD64"
    end
  end

  def install
    binary_name = "perch-#{OS.mac? ? "darwin" : "linux"}-#{Hardware::CPU.arm? ? "arm64" : "amd64"}"
    bin.install Dir["*"].first => "perch"
  end

  test do
    assert_match(/perch/i, shell_output("#{bin}/perch --help"))
    assert_match version.to_s, shell_output("#{bin}/perch --version")
  end
end
