# MT7603U driver
**Warning: there are still many bugs in this project, and welcome to join me
to improve it.**

## Build and Install
```bash
sudo apt install libelf-dev
make && sudo make install
```

Test it:
```
insmod mt7603u_sta
depmod
```

You may also want to update existing old driver:
```
rmmod mt7603u*
insmod mt7603u
depmod
```

## Vendor driver
**MT7603U_DPA_LinuxSTA_3.0.0.4_20140825**