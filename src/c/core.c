/* MT7603U Main Module Init & Exit */
#include <linux/module.h>
#include <linux/init.h>
#include <linux/usb.h>

MODULE_LICENSE("GPL");
MODULE_AUTHOR("Antigravity Agent");
MODULE_DESCRIPTION("MediaTek MT7603U Rust-assisted Linux WiFi Driver");
MODULE_VERSION("0.1.0");

extern struct usb_driver mt7603u_usb_driver;

static int __init mt7603u_init(void)
{
    pr_info("mt7603u: Initializing driver module (C skeleton + Rust logic staticlib)\n");
    return usb_register(&mt7603u_usb_driver);
}

static void __exit mt7603u_exit(void)
{
    pr_info("mt7603u: Unregistering driver module\n");
    usb_deregister(&mt7603u_usb_driver);
}

module_init(mt7603u_init);
module_exit(mt7603u_exit);
