{ pkgs, frameworks, RUSTFLAGS, craneLib }:

let
  # Define the common arguments that are shared between different packages
  src = craneLib.path ./..;
  crateName = craneLib.crateNameFromCargoToml { inherit src; };
  aptosCoreRepoUrl = "https://github.com/movementlabsxyz/aptos-core";
  isAptosCoreRepo = pkgs.lib.any (p: pkgs.lib.hasPrefix ("git+" + aptosCoreRepoUrl)  p.source);

  aptosCoreSrcsOverride = drv: drv.overrideAttrs (_old: {
      patches = [ ./aptos-relative-paths.patch ];
      postPatch = ''
          cp aptos-move/framework/src/aptos-natives.bpl third_party/move/move-prover/src/
          cp api/doc/{.version,spec.html} crates/aptos-faucet/core/src/endpoints/
          cp aptos-move/move-examples/scripts/minter/build/Minter/bytecode_scripts/main.mv \
              crates/aptos-faucet/core/src/funder/
      '';
  });

  cargoVendorDir = craneLib.vendorCargoDeps ( {
      inherit src;
      overrideVendorGitCheckout = ps: drv: if isAptosCoreRepo ps then aptosCoreSrcsOverride drv else drv;
  });

  commonArgs = {
      inherit src;
      strictDeps = true;
      doCheck = false;
      inherit (crateName) version;
      inherit cargoVendorDir;
      pname = "movement-services";

      nativeBuildInputs = [
          pkgs.pkg-config
          pkgs.clang
          pkgs.llvmPackages.bintools
          pkgs.protobuf_26
          pkgs.rustfmt
      ] ++ (pkgs.lib.optionals pkgs.stdenv.isLinux [ pkgs.systemd ]);
      buildInputs = [
          pkgs.openssl
          pkgs.libusb
      ] ++ (pkgs.lib.optionals pkgs.stdenv.isDarwin [
          pkgs.libiconv
          pkgs.darwin.IOKit
          pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
      ]);
      postPatch = ''
          mkdir -p "$TMPDIR/nix-vendor"
          cp -Lr "$cargoVendorDir" -T "$TMPDIR/nix-vendor"
          sed -i "s|$cargoVendorDir|$TMPDIR/nix-vendor/|g" "$TMPDIR/nix-vendor/config.toml"
          chmod -R +w "$TMPDIR/nix-vendor"
          cargoVendorDir="$TMPDIR/nix-vendor"
      '';
      LIBCLANG_PATH = "${pkgs.llvmPackages_18.libclang.lib}/lib";
      PKG_CONFIG_PATH = "${pkgs.libusb}/lib/pkgconfig";
      RUSTFLAGS = "${if RUSTFLAGS != null then RUSTFLAGS else ""} --cfg feature=\"vendored\"";
  };

  # Compute cargoArtifacts once
  cargoArtifacts = craneLib.buildDepsOnly commonArgs;

  # Function to build a package using the common arguments and cargoArtifacts
  buildPackage = packageName: craneLib.buildPackage (commonArgs // {
      inherit cargoArtifacts;
      pname = packageName;
      cargoExtraArgs = "--package ${packageName}";
      doNotRemoveReferencesToVendorDir = true;
  });

in
  {
    m1-da-light-node = buildPackage "m1-da-light-node";
    monza-config = buildPackage "monza-config";
    suzuka-config = buildPackage "monza-config";
    monza-full-node = buildPackage "monza-full-node";
    suzuka-full-node = buildPackage "suzuka-full-node";
    wait-for-celestia-light-node = buildPackage "wait-for-celestia-light-node";
    cargoArtifacts = cargoArtifacts;
  }
