# README

## What it does

> 📝 NOTE: Made for Raspberry pi zero 2 W, but could run on anything really

`rtsp_switcher.sh` runs `mpv` in the background and listens to IR remote key events to switch to a given RTSP stream.

Press **1**, **2**, or **3** on the remote to switch `mpv` to the corresponding RTSP stream via IPC.

## Prerequisites

Install packages:

* `socat`
* `mpv`
* `evtest`
* `ir-keytable`

```bash
sudo apt install socat mpv evtest ir-keytable
```

## Hardware

Wire an IR receiver (for example **TSOP4838**) to **GPIO17**.

## Enable gpio-ir

Add to `/boot/firmware/config.txt`:

```ini
# gpio-ir
dtoverlay=gpio-ir,gpio_pin=17
```

Reboot.

## Create an IR keymap (TOML)

Use `ir-keytable` to find the `gpio_ir_recv` device and its rc path (example: `rc0`). Then enable protocols, clear existing map, and test to discover scancodes:

```bash
sudo ir-keytable # Should show a gpio_ir_recv device (example: rc0)
sudo ir-keytable -s rc0 -p all
sudo ir-keytable -s rc0 -c
sudo ir-keytable -s rc0 -t
```

<details>
<summary>Output:</summary>

```bash
pi@raspberrypi:~ $ sudo ir-keytable
Found /sys/class/rc/rc1/ with:
        Name: vc4-hdmi
        Driver: cec
        Default keymap: rc-cec
        Input device: /dev/input/event1
        Supported kernel protocols: cec
        Enabled kernel protocols: cec
        bus: 30, vendor/product: 0000:0000, version: 0x0001
        Repeat delay: 0 ms, repeat period: 125 ms
Found /sys/class/rc/rc0/ with:
        Name: gpio_ir_recv
        Driver: gpio_ir_recv
        Default keymap: rc-rc6-mce
        Input device: /dev/input/event0
        LIRC device: /dev/lirc0
        Attached BPF protocols:
        Supported kernel protocols: lirc rc-5 rc-5-sz jvc sony nec sanyo mce_kbd rc-6 sharp xmp imon
        Enabled kernel protocols: lirc rc-5 rc-6
        bus: 25, vendor/product: 0001:0001, version: 0x0100
        Repeat delay: 500 ms, repeat period: 125 ms
pi@raspberrypi:~ $ sudo ir-keytable -s rc0 -p all
Protocols changed to unknown other lirc rc-5 rc-5-sz jvc sony nec sanyo mce_kbd rc-6 sharp xmp cec imon rc-mm
Loaded BPF protocol xbox-dvd
pi@raspberrypi:~ $ sudo ir-keytable -s rc0 -c
Old keytable cleared
pi@raspberrypi:~ $ sudo ir-keytable -s rc0 -t
Testing events. Please, press CTRL-C to abort.
2596.268023: lirc protocol(rc5): scancode = 0x1 toggle=1
2596.268043: event type EV_MSC(0x04): scancode = 0x01
2596.268043: event type EV_SYN(0x00).
2597.120024: lirc protocol(rc5): scancode = 0x2
2597.120042: event type EV_MSC(0x04): scancode = 0x02
2597.120042: event type EV_SYN(0x00).
2597.816026: lirc protocol(rc5): scancode = 0x3 toggle=1
2597.816049: event type EV_MSC(0x04): scancode = 0x03
2597.816049: event type EV_SYN(0x00).
```

</details>

<br>
Create a TOML keymap, for example `/etc/rc_keymaps/bush.toml`:

```toml
[[protocols]]
name = "bush"
protocol = "rc5"
variant = "rc5"

[protocols.scancodes]
0x00 = "KEY_NUMERIC_0"
0x01 = "KEY_NUMERIC_1"
0x02 = "KEY_NUMERIC_2"
0x03 = "KEY_NUMERIC_3"
0x04 = "KEY_NUMERIC_4"
0x05 = "KEY_NUMERIC_5"
0x06 = "KEY_NUMERIC_6"
0x07 = "KEY_NUMERIC_7"
0x08 = "KEY_NUMERIC_8"
0x09 = "KEY_NUMERIC_9"

0x0A = "KEY_A"
0x0B = "KEY_B"

0x0C = "KEY_POWER"
0x0D = "KEY_MUTE"
0x0E = "KEY_SOUND"
0x0F = "KEY_MENU"

0x10 = "KEY_VOLUMEUP"
0x11 = "KEY_VOLUMEDOWN"

0x20 = "KEY_CHANNELUP"
0x21 = "KEY_CHANNELDOWN"

0x24 = "KEY_LAST"

0x2A = "KEY_TIME"

0x32 = "KEY_YELLOW"
0x34 = "KEY_BLUE"
0x35 = "KEY_WHITE"
0x36 = "KEY_GREEN"
0x37 = "KEY_RED"

0x38 = "KEY_VIDEO"

0x3B = "KEY_MENU"
0x3C = "KEY_TEXT"
0x3F = "KEY_EXIT"
```

Add it to `/etc/rc_maps.cfg`:

```bash
*       *                        /etc/rc_keymaps/bush.toml
```

Load it (should auto-load on reboot):

```bash
sudo ir-keytable -a /etc/rc_maps.cfg -s rc0
```

## Verify key events with evtest

Run `evtest` and select the `gpio_ir_recv` input device (example: `/dev/input/event0`). Confirm keypresses show as `KEY_NUMERIC_1/2/3` with `value 1` for press:

```bash
evtest
```

<details>
<summary>Output:</summary>

```bash
pi@raspberrypi:~ $ evtest
No device specified, trying to scan all of /dev/input/event*
Not running as root, no devices may be available.
Available devices:
/dev/input/event0:      gpio_ir_recv
/dev/input/event1:      vc4-hdmi
/dev/input/event2:      vc4-hdmi HDMI Jack
Select the device event number [0-2]: 0
Input driver version is 1.0.1
Input device ID: bus 0x19 vendor 0x1 product 0x1 version 0x100
Input device name: "gpio_ir_recv"
Supported events:
  Event type 0 (EV_SYN)
  Event type 1 (EV_KEY)
    Event code 28 (KEY_ENTER)
    Event code 30 (KEY_A)
    Event code 48 (KEY_B)
    Event code 103 (KEY_UP)
    Event code 105 (KEY_LEFT)
    Event code 106 (KEY_RIGHT)
    Event code 108 (KEY_DOWN)
    Event code 111 (KEY_DELETE)
    Event code 113 (KEY_MUTE)
    Event code 114 (KEY_VOLUMEDOWN)
    Event code 115 (KEY_VOLUMEUP)
    Event code 116 (KEY_POWER)
    Event code 119 (KEY_PAUSE)
    Event code 128 (KEY_STOP)
    Event code 139 (KEY_MENU)
    Event code 142 (KEY_SLEEP)
    Event code 161 (KEY_EJECTCD)
    Event code 164 (KEY_PLAYPAUSE)
    Event code 167 (KEY_RECORD)
    Event code 168 (KEY_REWIND)
    Event code 174 (KEY_EXIT)
    Event code 207 (KEY_PLAY)
    Event code 208 (KEY_FASTFORWARD)
    Event code 210 (KEY_PRINT)
    Event code 212 (KEY_CAMERA)
    Event code 213 (KEY_SOUND)
    Event code 224 (KEY_BRIGHTNESSDOWN)
    Event code 225 (KEY_BRIGHTNESSUP)
    Event code 226 (KEY_MEDIA)
    Event code 352 (KEY_OK)
    Event code 356 (KEY_POWER2)
    Event code 358 (KEY_INFO)
    Event code 359 (KEY_TIME)
    Event code 365 (KEY_EPG)
    Event code 366 (KEY_PVR)
    Event code 368 (KEY_LANGUAGE)
    Event code 369 (KEY_TITLE)
    Event code 370 (KEY_SUBTITLE)
    Event code 372 (KEY_ZOOM)
    Event code 373 (KEY_MODE)
    Event code 377 (KEY_TV)
    Event code 385 (KEY_RADIO)
    Event code 386 (KEY_TUNER)
    Event code 387 (KEY_PLAYER)
    Event code 388 (KEY_TEXT)
    Event code 389 (KEY_DVD)
    Event code 392 (KEY_AUDIO)
    Event code 393 (KEY_VIDEO)
    Event code 398 (KEY_RED)
    Event code 399 (KEY_GREEN)
    Event code 400 (KEY_YELLOW)
    Event code 401 (KEY_BLUE)
    Event code 402 (KEY_CHANNELUP)
    Event code 403 (KEY_CHANNELDOWN)
    Event code 405 (KEY_LAST)
    Event code 407 (KEY_NEXT)
    Event code 412 (KEY_PREVIOUS)
    Event code 425 (KEY_PRESENTATION)
    Event code 430 (KEY_MESSENGER)
    Event code 512 (KEY_NUMERIC_0)
    Event code 513 (KEY_NUMERIC_1)
    Event code 514 (KEY_NUMERIC_2)
    Event code 515 (KEY_NUMERIC_3)
    Event code 516 (KEY_NUMERIC_4)
    Event code 517 (KEY_NUMERIC_5)
    Event code 518 (KEY_NUMERIC_6)
    Event code 519 (KEY_NUMERIC_7)
    Event code 520 (KEY_NUMERIC_8)
    Event code 521 (KEY_NUMERIC_9)
    Event code 522 (KEY_NUMERIC_STAR)
    Event code 523 (KEY_NUMERIC_POUND)
  Event type 2 (EV_REL)
    Event code 0 (REL_X)
    Event code 1 (REL_Y)
  Event type 4 (EV_MSC)
    Event code 4 (MSC_SCAN)
Key repeat handling:
  Repeat type 20 (EV_REP)
    Repeat code 0 (REP_DELAY)
      Value    500
    Repeat code 1 (REP_PERIOD)
      Value    125
Properties:
  Property type 5 (INPUT_PROP_POINTING_STICK)
Testing ... (interrupt to exit)
Event: time 1766326417.061928, type 4 (EV_MSC), code 4 (MSC_SCAN), value 01
Event: time 1766326417.061928, type 1 (EV_KEY), code 513 (KEY_NUMERIC_1), value 1
Event: time 1766326417.061928, -------------- SYN_REPORT ------------
Event: time 1766326417.177914, type 4 (EV_MSC), code 4 (MSC_SCAN), value 01
Event: time 1766326417.177914, -------------- SYN_REPORT ------------
Event: time 1766326417.313890, type 1 (EV_KEY), code 513 (KEY_NUMERIC_1), value 0
Event: time 1766326417.313890, -------------- SYN_REPORT ------------
Event: time 1766326417.829931, type 4 (EV_MSC), code 4 (MSC_SCAN), value 02
Event: time 1766326417.829931, type 1 (EV_KEY), code 514 (KEY_NUMERIC_2), value 1
Event: time 1766326417.829931, -------------- SYN_REPORT ------------
Event: time 1766326417.949927, type 4 (EV_MSC), code 4 (MSC_SCAN), value 02
Event: time 1766326417.949927, -------------- SYN_REPORT ------------
Event: time 1766326418.085887, type 1 (EV_KEY), code 514 (KEY_NUMERIC_2), value 0
Event: time 1766326418.085887, -------------- SYN_REPORT ------------
Event: time 1766326418.437926, type 4 (EV_MSC), code 4 (MSC_SCAN), value 03
Event: time 1766326418.437926, type 1 (EV_KEY), code 515 (KEY_NUMERIC_3), value 1
Event: time 1766326418.437926, -------------- SYN_REPORT ------------
Event: time 1766326418.557912, type 4 (EV_MSC), code 4 (MSC_SCAN), value 03
Event: time 1766326418.557912, -------------- SYN_REPORT ------------
Event: time 1766326418.693888, type 1 (EV_KEY), code 515 (KEY_NUMERIC_3), value 0
Event: time 1766326418.693888, -------------- SYN_REPORT ------------
```

</details>

## Configure the script

Edit the top-level variables:

* `DEV` (your `/dev/input/...` path for `gpio_ir_recv`)
* `URL_1`, `URL_2`, `URL_3` (your RTSP endpoints)

## Run

```bash
chmod +x ./rtsp_switcher.sh
./rtsp_switcher.sh
```

