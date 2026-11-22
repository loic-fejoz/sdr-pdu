.PHONY: esp32-local esp32-flash

esp32-local:
	@bash -c 'trap "kill 0" EXIT INT TERM; \
		./target/release/esp-pdu-tnc --serial-port /dev/ttyACM0 --kiss-port 18001 --kiss-ws-port 18002 --cat-port 14532 -f 2412000000 --peer-mac 48:27:e2:e4:74:2c & \
		./target/release/esp-pdu-tnc --serial-port /dev/ttyUSB0 --kiss-port 18011 --kiss-ws-port 18012 --cat-port 14542 -f 2412000000 --peer-mac fc:b4:67:74:32:84 & \
		wait'

esp32-flash:
	espflash flash --port /dev/ttyACM0 esp-pdu-firmware/target/xtensa-esp32-none-elf/release/esp-pdu-firmware
	espflash flash --port /dev/ttyUSB0 esp-pdu-firmware/target/xtensa-esp32s3-none-elf/release/esp-pdu-firmware
