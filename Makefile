TARGET = arm-unknown-linux-gnueabihf
CC = arm-linux-gnueabihf-gcc
CARGO = cargo

# Path to the PlutoSDR sysroot for cross-compilation.
# Can be overridden from command line: make SYSROOT=/path/to/sysroot release-pluto
SYSROOT ?= /home/loic/pluto-0.30.sysroot

# RUSTFLAGS for PlutoSDR:
# - target-cpu=cortex-a9
# - target-feature=+vfp3,+neon
# - link-arg=--sysroot: point to the target sysroot
# - L and rpath-link: ensure the linker finds the correct glibc/libiio in the sysroot
PLUTO_RUSTFLAGS = -C target-cpu=cortex-a9 -C target-feature=+vfp3,+neon \
                  -C link-arg=--sysroot=$(SYSROOT) \
                  -C link-arg=-L$(SYSROOT)/usr/lib \
                  -C link-arg=-L$(SYSROOT)/lib \
                  -C link-arg=-Wl,-rpath-link,$(SYSROOT)/usr/lib \
                  -C link-arg=-Wl,-rpath-link,$(SYSROOT)/lib

.PHONY: all clean release-pluto check test push

all: release-pluto

release-pluto:
	PKG_CONFIG_ALLOW_CROSS=1 \
	PKG_CONFIG_PATH=$(SYSROOT)/usr/lib/pkgconfig \
	CARGO_TARGET_ARM_UNKNOWN_LINUX_GNUEABIHF_LINKER=$(CC) \
	RUSTFLAGS="$(PLUTO_RUSTFLAGS)" \
	$(CARGO) build --release --target $(TARGET)

check:
	PKG_CONFIG_ALLOW_CROSS=1 \
	PKG_CONFIG_PATH=$(SYSROOT)/usr/lib/pkgconfig \
	CARGO_TARGET_ARM_UNKNOWN_LINUX_GNUEABIHF_LINKER=$(CC) \
	RUSTFLAGS="$(PLUTO_RUSTFLAGS)" \
	$(CARGO) check --target $(TARGET)

test:
	$(CARGO) test

clean:
	$(CARGO) clean

push: release-pluto
	scp -O ./target/arm-unknown-linux-gnueabihf/release/pluto-tx-2fsk root@192.168.2.1:/root/

push-spino: push
	scp -O spino.sh root@192.168.2.1:/root/
