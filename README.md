# MT7603U driver
**Warning: there are still many bugs in this project, and welcome to improve it.**

## Build and Install
```bash
sudo apt install libelf-dev
make && sudo make install
```

You may also want to update existing old driver:
```
rmmod mt7603u*
sudo make install
depmod
```

## Vendor driver
**MT7603U_DPA_LinuxSTA_3.0.0.4_20140825**
