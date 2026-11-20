#pragma once

namespace render::d3d12 {

// The device the vtable probes share. While a scope is open, the first probe makes the device and the holder keeps a
// reference of its own until the last scope closes, so every probe in between gets the same device; without an open
// scope each probe makes its own and the caller's release frees it. The caller locks around every call.
template <class Device> class ProbeHold {
public:
    template <class Make> Device *get(Make make)
    {
        if (held_) {
            held_->AddRef();
            return held_;
        }
        Device *dev = make();
        if (dev && scopes_) {
            dev->AddRef();
            held_ = dev;
        }
        return dev;
    }

    void open() { ++scopes_; }

    bool close()
    {
        if (--scopes_ || !held_)
            return false;
        held_->Release();
        held_ = nullptr;
        return true;
    }

    bool holding() const { return held_ != nullptr; }
    int scopes() const { return scopes_; }

private:
    Device *held_ = nullptr;
    int scopes_ = 0;
};

}
