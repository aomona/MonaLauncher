-- Pass an already resized ARM64 qcow2 image and a NoCloud seed ISO.
-- UTM 4.7.4 ignores guest size when importing an existing image.
on run argv
    if (count argv) is not 2 then error "usage: osascript create.applescript disk.qcow2 seed.iso"
    set diskImage to POSIX file (item 1 of argv)
    set seedImage to POSIX file (item 2 of argv)
    tell application "UTM"
        set vm to make new virtual machine with properties {backend:qemu, configuration:{name:"MonaLauncher Linux Desktop", architecture:"aarch64", memory:6144, cpu cores:4, hypervisor:true, uefi:true, directory share mode:none, network interfaces:{{mode:emulated, hardware:"virtio-net-pci", address:"06:E1:86:AA:22:60"}}, displays:{{hardware:"virtio-ramfb", dynamic resolution:false}}, drives:{{removable:false, interface:VirtIO, source:diskImage}, {removable:false, interface:VirtIO, source:seedImage}}}}
        return id of vm
    end tell
end run
