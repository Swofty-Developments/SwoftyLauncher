#pragma once

namespace render::com {

template <class T> void release(T *&p)
{
    if (p)
        p->Release();
    p = nullptr;
}

}
