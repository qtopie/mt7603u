# MT7603U Kbuild Makefile
KDIR ?= /lib/modules/$(shell uname -r)/build
PWD := $(shell pwd)

RUST_DIR := $(PWD)/src/rust
RUST_LIB := $(RUST_DIR)/target/x86_64-unknown-none/release/libmt7603u_logic.a
RUST_OBJ := $(PWD)/src/c/rust_logic.o

obj-m := mt7603u.o
mt7603u-objs := src/c/core.o src/c/usb.o src/c/mac80211.o src/c/regops.o src/c/rust_logic.o

# Disable objtool check for Rust staticlib objects
OBJECT_FILES_NON_STANDARD := y
OBJECT_FILES_NON_STANDARD_mt7603u.o := y
OBJECT_FILES_NON_STANDARD_src/c/rust_logic.o := y

ccflags-y += -I$(PWD)/src/c

all: rust_build
	$(MAKE) -C $(KDIR) M=$(PWD) CONFIG_OBJTOOL= modules
	@if [ -f /var/lib/shim-signed/mok/MOK.priv ]; then \
		echo "Signing mt7603u.ko with MOK key..."; \
		cat ~/.pass | sudo -S /usr/src/linux-headers-$(shell uname -r)/scripts/sign-file sha256 /var/lib/shim-signed/mok/MOK.priv /var/lib/shim-signed/mok/MOK.der mt7603u.ko; \
	fi

rust_build:
	cd $(RUST_DIR) && RUSTFLAGS="-C code-model=kernel -C relocation-model=static -C no-redzone" cargo build --release --target x86_64-unknown-none
	$(LD) -r --gc-sections \
		-u mt7603_rust_parse_eeprom \
		-u mt7603_rust_map_register_addr \
		-u mt7603_rust_get_mac_init_sequence \
		-u mt7603_rust_build_own_mac_sequence \
		-u mt7603_rust_get_channel_sequence \
		-u mt7603_rust_parse_rx_frame \
		-u mt7603_rust_build_txwi \
		-u mt7603_rust_build_addr_len_req \
		-u mt7603_rust_build_fw_start_req \
		-u mt7603_rust_build_restart_dl_req \
		-u mt7603_rust_build_fw_scatter_frame \
		-u mt7603_rust_fw_dl_len \
		-u mt7603_rust_verify_firmware \
		-u mt7603_rust_build_probe_req \
		-u mt7603_rust_parse_beacon \
		-u mt7603_rust_build_beacon \
		-u mt7603_rust_build_assoc_resp \
		-u mt7603_rust_parse_assoc_req \
		-u mt7603_rust_build_chan_switch_cmd \
		-u mt7603_rust_build_tx_power_ctrl_cmd \
		-u mt7603_rust_build_ch_privilege_cmd \
		-u mt7603_rust_build_radio_on_off_cmd \
		-u mt7603_rust_build_efuse_buffer_mode_cmd \
		--whole-archive $(RUST_LIB) -o $(RUST_OBJ)
	python3 -c 'with open("$(RUST_OBJ)", "rb") as f: d=bytearray(f.read()); [d.__setitem__(slice(o+8,o+12), (4).to_bytes(4, "little")) for o in range(0, len(d)-24, 8) if int.from_bytes(d[o+8:o+12], "little")==9]; open("$(RUST_OBJ)", "wb").write(d)'
	objcopy -R .llvmbc -R .llvmcmd $(RUST_OBJ)
	echo "cmd_$(RUST_OBJ) := true" > $(PWD)/src/c/.rust_logic.o.cmd

clean:
	$(MAKE) -C $(KDIR) M=$(PWD) clean
	cd $(RUST_DIR) && cargo clean
	rm -f $(RUST_OBJ) $(PWD)/src/c/.rust_logic.o.cmd


