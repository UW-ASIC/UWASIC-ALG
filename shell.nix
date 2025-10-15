{pkgs ? import <nixpkgs> {}}: let
  # ngspice with shared library enabled
  ngspice-shared = pkgs.ngspice.override {
    withNgshared = true;
  };

  xschem = pkgs.stdenv.mkDerivation rec {
    pname = "xschem";
    version = "3.4.6";
    src = pkgs.fetchFromGitHub {
      owner = "StefanSchippers";
      repo = "xschem";
      rev = "3.4.6";
      sha256 = "sha256-1jP1SJeq23XNkOQgcl2X+rBrlka4a04irmfhoKRM1j4=";
    };
    nativeBuildInputs = with pkgs; [
      pkg-config
      autoconf
      automake
    ];
    buildInputs = with pkgs; [
      tcl
      tk
      xorg.libX11
      xorg.libXpm
      cairo
      readline
      flex
      bison
      zlib
    ];
    configureFlags = [
      "--prefix=${placeholder "out"}"
    ];
    buildPhase = ''
      make
    '';

    installPhase = ''
      make install
    '';
    meta = {
      description = "Schematic capture and netlisting EDA tool";
      homepage = "https://xschem.sourceforge.io/";
      platforms = pkgs.lib.platforms.linux;
    };
  };

  netgen-old = pkgs.stdenv.mkDerivation rec {
    name = "netgen";
    version = "1.5.295";
    src = pkgs.fetchurl {
      url = "http://opencircuitdesign.com/netgen/archive/netgen-${version}.tgz";
      sha256 = "sha256-y2UBf564WefrDbIxSrFbNc1FxQfDdYzRORrJjRdkKrg=";
    };
    nativeBuildInputs = [pkgs.python3];
    buildInputs = with pkgs; [
      tcl
      tk
      xorg.libX11
    ];
    enableParallelBuilding = true;
    configureFlags = [
      "--with-tcl=${pkgs.tcl}"
      "--with-tk=${pkgs.tk}"
    ];
    postPatch = ''
      find . -name "*.sh" -exec patchShebangs {} \; || true
    '';
    meta = with pkgs.lib; {
      description = "LVS netlist comparison tool";
      homepage = "http://opencircuitdesign.com/netgen/";
      license = pkgs.lib.licenses.mit;
      maintainers = with pkgs.lib.maintainers; [];
    };
  };
in
  pkgs.mkShell {
    buildInputs = with pkgs; [
      # Rust development
      rustup
      cargo
      git
      gnumake
      pkg-config

      # C compilation dependencies
      gcc
      glibc.dev
      libffi.dev
      clang
      llvmPackages.libclang

      # Python with full development headers
      python311Full
      python311Packages.pip
      python311Packages.virtualenv

      # Building Dependencies for Testing
      xschem
      ngspice-shared # Use the shared library version
      # fonts
      xorg.libX11
      xorg.libXpm
      xorg.libXt
      cairo
      xterm
      xorg.fontutil
      xorg.fontmiscmisc
      xorg.fontcursormisc
      dejavu_fonts
      liberation_ttf
    ];

    shellHook = ''
      export PROJECT_ROOT="$(pwd)"
      export PDK_ROOT="$HOME/.volare"
      export PDK="sky130A"
      export XSCHEM_USER_LIBRARY_PATH="$PDK_ROOT/$PDK/libs.tech/xschem"
      export XSCHEM_LIBRARY_PATH="$PDK_ROOT/$PDK/libs.tech/xschem:${xschem}/share/xschem/xschem_library"

      # Set up Rust nightly
      export RUSTUP_HOME="$HOME/.rustup"
      export CARGO_HOME="$HOME/.cargo"
      export PATH="$CARGO_HOME/bin:$PATH"

      # Environment for bindgen
      export LIBCLANG_PATH="${pkgs.llvmPackages.libclang.lib}/lib"
      export BINDGEN_EXTRA_CLANG_ARGS="-I${pkgs.glibc.dev}/include -I${ngspice-shared}/include"

      # Python and C compilation paths
      export CPATH="${pkgs.python311Full}/include/python3.11:${ngspice-shared}/include:$CPATH"
      export NIX_LD_LIBRARY_PATH="${pkgs.python311Full}/lib:${ngspice-shared}/lib:$NIX_LD_LIBRARY_PATH"
      export PKG_CONFIG_PATH="${ngspice-shared}/lib/pkgconfig:$PKG_CONFIG_PATH"

      # Setup Python virtual environment
      export VENV_DIR="$PROJECT_ROOT/.venv"
      if [ ! -d "$VENV_DIR" ]; then
        echo "Creating Python virtual environment..."
        python -m venv "$VENV_DIR"
      fi
      source "$VENV_DIR/bin/activate"

      # Install Rust nightly if not already installed
      if ! rustc --version &>/dev/null; then
        echo "Installing Rust nightly toolchain..."
        rustup install nightly
        rustup default nightly
      fi

      # Install maturin if not already installed
      if ! pip show maturin &>/dev/null; then
        echo "Installing maturin..."
        pip install maturin
      fi

      # Install pytest if not already installed
      if ! pip show pytest &>/dev/null; then
        echo "Installing pytest..."
        pip install pytest
      fi

      echo "UWASIC Circuit Optimizer (Rust) development environment loaded!"
      echo ""
      echo "Available tools:"
      echo "  - rustc: $(rustc --version 2>/dev/null || echo 'not installed')"
      echo "  - cargo: $(cargo --version 2>/dev/null || echo 'not installed')"
      echo "  - python: $(python --version)"
      echo "  - ngspice: $(ngspice --version 2>/dev/null | head -1 || echo 'unknown version')"
      echo "  - ngspice lib: ${ngspice-shared}/lib"
      echo "  - xschem: $(xschem --version 2>/dev/null || echo 'custom build')"
      echo "  - PDK: $PDK in $PDK_ROOT"
      echo ""
      echo "To build and test:"
      echo "  maturin develop              # Build and install Python bindings"
      echo "  pytest test/ -v              # Run tests"
      echo "  python examples/optimizer.py # Run example"
    '';

    # Environment variables for Python C extension compilation
    NIX_LDFLAGS = "-L${pkgs.python311Full}/lib -L${pkgs.libffi}/lib -L${ngspice-shared}/lib";
  }
