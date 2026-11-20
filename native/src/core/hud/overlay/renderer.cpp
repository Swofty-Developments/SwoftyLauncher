#include "core/hud/overlay/renderer.hpp"

#include "core/render/d3d12_hooks.hpp"

#include <swofty/mixin.hpp>

#include <algorithm>
#include <climits>
#include <cmath>
#include <cstring>

namespace hud::overlay {

bool Renderer::compile(pD3DCompile fn, const char *entry, const char *model, ID3DBlob **out)
{
    static constexpr char hlsl[] =
        "cbuffer C:register(b0){float2 inv;}"
        "Texture2D T:register(t1);SamplerState S:register(s0);"
        "struct I{float2 p:POSITION;float2 t:TEXCOORD;float4 c:COLOR;uint m:MODE;};"
        "struct O{float4 p:SV_POSITION;float2 t:TEXCOORD;float4 c:COLOR;nointerpolation uint m:MODE;};"
        "O VS(I i){O o;o.p=float4(i.p.x*inv.x-1.0,1.0-i.p.y*inv.y,0,1);o.t=i.t;o.c=i.c;o.m=i.m;return o;}"
        "float4 PS(O i):SV_TARGET{float4 s=T.Sample(S,i.t);float w=max(fwidth(s.r),1e-5);uint k=i.m&255;"
        "float4 t=float4(1,1,1,1);if(k==1)t=s;else if(k==2){float e=0.5-float((i.m>>8)&255)/510.0;"
        "float h=w*(0.5+float((i.m>>16)&255)/16.0);float a=smoothstep(e-h,e+h,s.r);t=float4(a,a,a,a);}"
        "return float4(t.rgb*i.c.rgb*i.c.a,t.a*i.c.a);}"
        "Texture2D L:register(t0);"
        "cbuffer K:register(b1){uint mode;float white;}"
        "float4 CVS(uint id:SV_VertexID):SV_POSITION{float2 t=float2((id<<1)&2,id&2);"
        "return float4(t*float2(2,-2)+float2(-1,1),0,1);}"
        "float3 lin(float3 c){return c<=0.04045?c/12.92:pow((c+0.055)/1.055,2.4);}"
        "float3 pq(float3 n){float3 y=pow(max(n/10000.0,0),0.1593017578125);"
        "return pow((0.8359375+18.8515625*y)/(1+18.6875*y),78.84375);}"
        "float4 CPS(float4 p:SV_POSITION):SV_TARGET{float4 v=L.Load(int3(p.xy,0));if(v.a<=0.0)discard;"
        "float3 c=v.rgb/v.a;"
        "if(mode==1){float3 l=lin(c);l=float3(dot(l,float3(0.6274040,0.3292820,0.0433136)),"
        "dot(l,float3(0.0690970,0.9195400,0.0113612)),dot(l,float3(0.0163916,0.0880132,0.8955950)));c=pq(l*white);}"
        "else if(mode==2){c=lin(c)*(white/80.0);}"
        "return float4(c*v.a,v.a);}";
    ID3DBlob *err = nullptr;
    HRESULT hr = fn(hlsl, sizeof hlsl - 1, nullptr, nullptr, nullptr, entry, model, 0, 0, out, &err);
    if (err) {
        if (FAILED(hr))
            swofty::log("[overlay] %s: %.*s", entry, int(err->GetBufferSize()),
                        static_cast<const char *>(err->GetBufferPointer()));
        err->Release();
    }
    return SUCCEEDED(hr) && *out;
}

ID3D12RootSignature *Renderer::make_sig(const D3D12_ROOT_SIGNATURE_DESC &rs)
{
    ID3DBlob *blob = nullptr, *rserr = nullptr;
    ID3D12RootSignature *sig = nullptr;
    HRESULT hr = serialize_root(&rs, D3D_ROOT_SIGNATURE_VERSION_1, &blob, &rserr);
    if (rserr)
        rserr->Release();
    if (SUCCEEDED(hr))
        hr = dev->CreateRootSignature(0, blob->GetBufferPointer(), blob->GetBufferSize(), IID_PPV_ARGS(&sig));
    release(blob);
    return SUCCEEDED(hr) ? sig : nullptr;
}

bool Renderer::make_root()
{
    HMODULE compiler = LoadLibraryA("d3dcompiler_47.dll");
    auto compile_fn = compiler ? reinterpret_cast<pD3DCompile>(GetProcAddress(compiler, "D3DCompile")) : nullptr;
    if (!compile_fn) {
        swofty::log("[overlay] d3dcompiler_47.dll!D3DCompile is unavailable; overlay rendering off");
        return false;
    }
    if (!compile(compile_fn, "VS", "vs_5_0", &vs) || !compile(compile_fn, "PS", "ps_5_0", &ps) ||
        !compile(compile_fn, "CVS", "vs_5_0", &cvs) || !compile(compile_fn, "CPS", "ps_5_0", &cps)) {
        swofty::log("[overlay] overlay shader compilation failed");
        return false;
    }
    D3D12_DESCRIPTOR_RANGE trange{};
    trange.RangeType = D3D12_DESCRIPTOR_RANGE_TYPE_SRV;
    trange.NumDescriptors = 1;
    trange.BaseShaderRegister = 1;
    D3D12_ROOT_PARAMETER rp[2]{};
    rp[0].ParameterType = D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS;
    rp[0].Constants.Num32BitValues = 2;
    rp[0].ShaderVisibility = D3D12_SHADER_VISIBILITY_VERTEX;
    rp[1].ParameterType = D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE;
    rp[1].DescriptorTable.NumDescriptorRanges = 1;
    rp[1].DescriptorTable.pDescriptorRanges = &trange;
    rp[1].ShaderVisibility = D3D12_SHADER_VISIBILITY_PIXEL;
    D3D12_STATIC_SAMPLER_DESC samp{};
    samp.Filter = D3D12_FILTER_MIN_MAG_MIP_LINEAR;
    samp.AddressU = samp.AddressV = samp.AddressW = D3D12_TEXTURE_ADDRESS_MODE_CLAMP;
    samp.MaxLOD = D3D12_FLOAT32_MAX;
    samp.ShaderVisibility = D3D12_SHADER_VISIBILITY_PIXEL;
    D3D12_ROOT_SIGNATURE_DESC rs{};
    rs.NumParameters = 2;
    rs.pParameters = rp;
    rs.NumStaticSamplers = 1;
    rs.pStaticSamplers = &samp;
    rs.Flags = D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT;
    root = make_sig(rs);
    D3D12_DESCRIPTOR_RANGE range{};
    range.RangeType = D3D12_DESCRIPTOR_RANGE_TYPE_SRV;
    range.NumDescriptors = 1;
    D3D12_ROOT_PARAMETER cp[2]{};
    cp[0].ParameterType = D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS;
    cp[0].Constants.ShaderRegister = 1;
    cp[0].Constants.Num32BitValues = 2;
    cp[0].ShaderVisibility = D3D12_SHADER_VISIBILITY_PIXEL;
    cp[1].ParameterType = D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE;
    cp[1].DescriptorTable.NumDescriptorRanges = 1;
    cp[1].DescriptorTable.pDescriptorRanges = &range;
    cp[1].ShaderVisibility = D3D12_SHADER_VISIBILITY_PIXEL;
    D3D12_ROOT_SIGNATURE_DESC crs{};
    crs.NumParameters = 2;
    crs.pParameters = cp;
    comp_root = make_sig(crs);
    return root && comp_root && (geo = make_pso(LAYER_FMT, false));
}

ID3D12PipelineState *Renderer::make_pso(DXGI_FORMAT fmt, bool composite)
{
    D3D12_INPUT_ELEMENT_DESC layout[] = {
        {"POSITION", 0, DXGI_FORMAT_R32G32_FLOAT, 0, 0, D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA, 0},
        {"TEXCOORD", 0, DXGI_FORMAT_R32G32_FLOAT, 0, 8, D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA, 0},
        {"COLOR", 0, DXGI_FORMAT_R8G8B8A8_UNORM, 0, 16, D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA, 0},
        {"MODE", 0, DXGI_FORMAT_R32_UINT, 0, 20, D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA, 0}};
    D3D12_GRAPHICS_PIPELINE_STATE_DESC pd{};
    pd.pRootSignature = composite ? comp_root : root;
    ID3DBlob *v = composite ? cvs : vs, *p = composite ? cps : ps;
    pd.VS = {v->GetBufferPointer(), v->GetBufferSize()};
    pd.PS = {p->GetBufferPointer(), p->GetBufferSize()};
    if (!composite)
        pd.InputLayout = {layout, 4};
    pd.PrimitiveTopologyType = D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE;
    pd.NumRenderTargets = 1;
    pd.RTVFormats[0] = fmt;
    pd.SampleDesc.Count = 1;
    pd.SampleMask = UINT_MAX;
    pd.RasterizerState.FillMode = D3D12_FILL_MODE_SOLID;
    pd.RasterizerState.CullMode = D3D12_CULL_MODE_NONE;
    auto &rt = pd.BlendState.RenderTarget[0];
    rt.BlendEnable = TRUE;
    rt.SrcBlend = D3D12_BLEND_ONE;
    rt.DestBlend = D3D12_BLEND_INV_SRC_ALPHA;
    rt.BlendOp = D3D12_BLEND_OP_ADD;
    rt.SrcBlendAlpha = D3D12_BLEND_ONE;
    rt.DestBlendAlpha = D3D12_BLEND_INV_SRC_ALPHA;
    rt.BlendOpAlpha = D3D12_BLEND_OP_ADD;
    rt.RenderTargetWriteMask = D3D12_COLOR_WRITE_ENABLE_ALL;
    ID3D12PipelineState *pso = nullptr;
    return SUCCEEDED(dev->CreateGraphicsPipelineState(&pd, IID_PPV_ARGS(&pso))) ? pso : nullptr;
}

ID3D12PipelineState *Renderer::pso_for(DXGI_FORMAT fmt)
{
    for (auto &p : psos)
        if (p.fmt == fmt)
            return p.pso;
    ID3D12PipelineState *pso = make_pso(fmt, true);
    if (pso)
        psos.push_back({fmt, pso});
    return pso;
}

ID3D12Resource *Renderer::buffer(D3D12_HEAP_TYPE type, UINT64 size, D3D12_RESOURCE_STATES state)
{
    D3D12_HEAP_PROPERTIES hp{};
    hp.Type = type;
    D3D12_RESOURCE_DESC rd{};
    rd.Dimension = D3D12_RESOURCE_DIMENSION_BUFFER;
    rd.Width = size;
    rd.Height = 1;
    rd.DepthOrArraySize = 1;
    rd.MipLevels = 1;
    rd.Format = DXGI_FORMAT_UNKNOWN;
    rd.SampleDesc.Count = 1;
    rd.Layout = D3D12_TEXTURE_LAYOUT_ROW_MAJOR;
    ID3D12Resource *r = nullptr;
    return SUCCEEDED(dev->CreateCommittedResource(&hp, D3D12_HEAP_FLAG_NONE, &rd, state, nullptr, IID_PPV_ARGS(&r))) ? r
                                                                                                                     : nullptr;
}

bool Renderer::make_resources(ID3D12CommandQueue *q)
{
    for (auto &s : slots) {
        if (FAILED(dev->CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT, IID_PPV_ARGS(&s.alloc))))
            return false;
        if (FAILED(dev->CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, s.alloc, nullptr, IID_PPV_ARGS(&s.list))))
            return false;
        s.list->Close();
    }
    if (FAILED(dev->CreateFence(0, D3D12_FENCE_FLAG_NONE, IID_PPV_ARGS(&fence))))
        return false;
    D3D12_DESCRIPTOR_HEAP_DESC hd{};
    hd.Type = D3D12_DESCRIPTOR_HEAP_TYPE_RTV;
    hd.NumDescriptors = SLOTS * 2;
    if (FAILED(dev->CreateDescriptorHeap(&hd, IID_PPV_ARGS(&rtv_heap))))
        return false;
    rtv_inc = dev->GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_RTV);
    D3D12_DESCRIPTOR_HEAP_DESC sd{};
    sd.Type = D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV;
    sd.NumDescriptors = SLOTS + TEX_MAX;
    sd.Flags = D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE;
    if (FAILED(dev->CreateDescriptorHeap(&sd, IID_PPV_ARGS(&srv_heap))))
        return false;
    srv_inc = dev->GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV);
    D3D12_SHADER_RESOURCE_VIEW_DESC null_srv{};
    null_srv.Format = DXGI_FORMAT_R8G8B8A8_UNORM;
    null_srv.ViewDimension = D3D12_SRV_DIMENSION_TEXTURE2D;
    null_srv.Shader4ComponentMapping = D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING;
    null_srv.Texture2D.MipLevels = 1;
    for (UINT i = 0; i < TEX_MAX; i++)
        dev->CreateShaderResourceView(nullptr, &null_srv, tex_cpu(i));
    textures.reset();
    vbuf = buffer(D3D12_HEAP_TYPE_UPLOAD, UINT64(sizeof(Vertex)) * MAX_VERTS * SLOTS, D3D12_RESOURCE_STATE_GENERIC_READ);
    D3D12_RANGE none{0, 0};
    if (!vbuf || FAILED(vbuf->Map(0, &none, reinterpret_cast<void **>(&vmap))))
        return false;
    D3D12_QUERY_HEAP_DESC qd{};
    qd.Type = D3D12_QUERY_HEAP_TYPE_TIMESTAMP;
    qd.Count = SLOTS * 2;
    if (SUCCEEDED(q->GetTimestampFrequency(&ts_freq)) && ts_freq && SUCCEEDED(dev->CreateQueryHeap(&qd, IID_PPV_ARGS(&ts_heap))))
        ts_buf = buffer(D3D12_HEAP_TYPE_READBACK, SLOTS * 16, D3D12_RESOURCE_STATE_COPY_DEST);
    if (!ts_buf)
        release(ts_heap);
    return true;
}

D3D12_CPU_DESCRIPTOR_HANDLE Renderer::tex_cpu(UINT i)
{
    D3D12_CPU_DESCRIPTOR_HANDLE h = srv_heap->GetCPUDescriptorHandleForHeapStart();
    h.ptr += SIZE_T(SLOTS + i) * srv_inc;
    return h;
}

D3D12_GPU_DESCRIPTOR_HANDLE Renderer::tex_gpu(UINT i)
{
    D3D12_GPU_DESCRIPTOR_HANDLE h = srv_heap->GetGPUDescriptorHandleForHeapStart();
    h.ptr += UINT64(SLOTS + i) * srv_inc;
    return h;
}

bool Renderer::same_device(ID3D12CommandQueue *q)
{
    if (std::find(queues.begin(), queues.end(), q) != queues.end())
        return true;
    ID3D12Device *d = nullptr;
    if (FAILED(q->GetDevice(IID_PPV_ARGS(&d))) || !d)
        return false;
    bool same = d == dev;
    d->Release();
    if (same)
        queues.push_back(q);
    return same;
}

bool Renderer::init(ID3D12CommandQueue *q)
{
    if (FAILED(q->GetDevice(IID_PPV_ARGS(&dev))))
        return false;
    if (!make_root() || !make_resources(q))
        return false;
    ready = true;
    return true;
}

void Renderer::bury()
{
    UINT64 done = fence->GetCompletedValue();
    for (size_t i = 0; i < graveyard.size();)
        if (graveyard[i].first <= done) {
            graveyard[i].second->Release();
            graveyard[i] = graveyard.back();
            graveyard.pop_back();
        } else {
            i++;
        }
}

bool Renderer::ensure_layer(UINT w, UINT h)
{
    if (layer && layer_w == w && layer_h == h)
        return true;
    if (layer)
        graveyard.push_back({fence_val, layer});
    layer = nullptr;
    D3D12_HEAP_PROPERTIES hp{};
    hp.Type = D3D12_HEAP_TYPE_DEFAULT;
    D3D12_RESOURCE_DESC rd{};
    rd.Dimension = D3D12_RESOURCE_DIMENSION_TEXTURE2D;
    rd.Width = w;
    rd.Height = h;
    rd.DepthOrArraySize = 1;
    rd.MipLevels = 1;
    rd.Format = LAYER_FMT;
    rd.SampleDesc.Count = 1;
    rd.Flags = D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET;
    D3D12_CLEAR_VALUE cv{};
    cv.Format = LAYER_FMT;
    if (FAILED(dev->CreateCommittedResource(&hp, D3D12_HEAP_FLAG_NONE, &rd, D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE, &cv,
                                            IID_PPV_ARGS(&layer))))
        return false;
    layer_w = w;
    layer_h = h;
    layer_state = D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE;
    return true;
}

void Renderer::read_timestamps(UINT slot)
{
    if (!ts_buf || !slots[slot].done_at)
        return;
    D3D12_RANGE r{SIZE_T(slot) * 16, SIZE_T(slot) * 16 + 16};
    uint64_t *t = nullptr;
    if (FAILED(ts_buf->Map(0, &r, reinterpret_cast<void **>(&t))) || !t)
        return;
    uint64_t a = t[slot * 2], b = t[slot * 2 + 1];
    D3D12_RANGE none{0, 0};
    ts_buf->Unmap(0, &none);
    if (b > a) {
        gpu_ms += double(b - a) * 1000.0 / double(ts_freq);
        gpu_n++;
    }
}

void Renderer::transition(ID3D12GraphicsCommandList *l, ID3D12Resource *r, D3D12_RESOURCE_STATES from, D3D12_RESOURCE_STATES to)
{
    D3D12_RESOURCE_BARRIER b{};
    b.Transition.pResource = r;
    b.Transition.StateBefore = from;
    b.Transition.StateAfter = to;
    b.Transition.Subresource = D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES;
    l->ResourceBarrier(1, &b);
}

Renderer::Outcome Renderer::render(const Target &t, ID3D12CommandQueue *q, const std::vector<Vertex> &verts,
                                   const std::vector<hud::drawlist::Batch> &batches, UINT mode, float white)
{
    const DXGI_FORMAT fmt = t.fmt;
    const UINT width = t.width, height = t.height;
    ID3D12Resource *buf = t.res;
    Slot &s = slots[next];
    if (fence->GetCompletedValue() < s.done_at)
        return busy;
    bury();
    read_timestamps(next);
    size_t n = std::min<size_t>(verts.size(), MAX_VERTS);
    float x0 = float(width), y0 = float(height), x1 = 0, y1 = 0;
    for (size_t i = 0; i < n; i++) {
        x0 = std::min(x0, verts[i].x), y0 = std::min(y0, verts[i].y);
        x1 = std::max(x1, verts[i].x), y1 = std::max(y1, verts[i].y);
    }
    D3D12_RECT box{LONG(std::max(0.0f, std::floor(x0) - 1)), LONG(std::max(0.0f, std::floor(y0) - 1)),
                   LONG(std::min(float(width), std::ceil(x1) + 1)), LONG(std::min(float(height), std::ceil(y1) + 1))};
    if (box.right <= box.left || box.bottom <= box.top)
        return drawn;
    ID3D12PipelineState *pso = pso_for(fmt);
    if (!pso || !ensure_layer(width, height))
        return broken;
    size_t base = size_t(next) * MAX_VERTS * sizeof(Vertex);
    memcpy(vmap + base, verts.data(), n * sizeof(Vertex));
    if (FAILED(s.alloc->Reset()) || FAILED(s.list->Reset(s.alloc, geo)))
        return broken;
    ID3D12GraphicsCommandList *l = s.list;
    if (ts_heap)
        l->EndQuery(ts_heap, D3D12_QUERY_TYPE_TIMESTAMP, next * 2);
    textures.sync(*this, l);
    D3D12_CPU_DESCRIPTOR_HANDLE rtv = rtv_heap->GetCPUDescriptorHandleForHeapStart(), lrtv = rtv;
    rtv.ptr += SIZE_T(next) * 2 * rtv_inc;
    lrtv.ptr += (SIZE_T(next) * 2 + 1) * rtv_inc;
    D3D12_RENDER_TARGET_VIEW_DESC rv{};
    rv.Format = fmt;
    rv.ViewDimension = D3D12_RTV_DIMENSION_TEXTURE2D;
    dev->CreateRenderTargetView(buf, &rv, rtv);
    dev->CreateRenderTargetView(layer, nullptr, lrtv);
    D3D12_CPU_DESCRIPTOR_HANDLE srv = srv_heap->GetCPUDescriptorHandleForHeapStart();
    srv.ptr += SIZE_T(next) * srv_inc;
    dev->CreateShaderResourceView(layer, nullptr, srv);
    D3D12_GPU_DESCRIPTOR_HANDLE gsrv = srv_heap->GetGPUDescriptorHandleForHeapStart();
    gsrv.ptr += UINT64(next) * srv_inc;
    if (layer_state != D3D12_RESOURCE_STATE_RENDER_TARGET)
        transition(l, layer, layer_state, D3D12_RESOURCE_STATE_RENDER_TARGET);
    layer_state = D3D12_RESOURCE_STATE_RENDER_TARGET;
    const float clear[4] = {0, 0, 0, 0};
    l->OMSetRenderTargets(1, &lrtv, FALSE, nullptr);
    l->ClearRenderTargetView(lrtv, clear, 1, &box);
    D3D12_VIEWPORT vp{0, 0, float(width), float(height), 0, 1};
    l->RSSetViewports(1, &vp);
    l->RSSetScissorRects(1, &box);
    l->SetGraphicsRootSignature(root);
    float inv[2] = {2.0f / float(width ? width : 1), 2.0f / float(height ? height : 1)};
    l->SetGraphicsRoot32BitConstants(0, 2, inv, 0);
    l->IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
    D3D12_VERTEX_BUFFER_VIEW vbv{vbuf->GetGPUVirtualAddress() + base, UINT(n * sizeof(Vertex)), sizeof(Vertex)};
    l->IASetVertexBuffers(0, 1, &vbv);
    l->SetDescriptorHeaps(1, &srv_heap);
    for (const hud::drawlist::Batch &b : batches) {
        if (b.first >= n)
            break;
        D3D12_RECT sr{std::max(box.left, LONG(std::floor(b.clip.x0))), std::max(box.top, LONG(std::floor(b.clip.y0))),
                      std::min(box.right, LONG(std::ceil(b.clip.x1))), std::min(box.bottom, LONG(std::ceil(b.clip.y1)))};
        if (sr.right <= sr.left || sr.bottom <= sr.top)
            continue;
        UINT tex = b.tex >= 0 && UINT(b.tex) < TEX_MAX ? UINT(b.tex) : 0;
        l->SetGraphicsRootDescriptorTable(1, tex_gpu(tex));
        l->RSSetScissorRects(1, &sr);
        l->DrawInstanced(std::min<UINT>(b.count, UINT(n) - b.first), 1, b.first, 0);
    }
    D3D12_RESOURCE_BARRIER two[2]{};
    two[0].Transition = {layer, D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES, D3D12_RESOURCE_STATE_RENDER_TARGET,
                         D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE};
    two[1].Transition = {buf, D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES, t.state, D3D12_RESOURCE_STATE_RENDER_TARGET};
    l->ResourceBarrier(2, two);
    layer_state = D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE;
    l->OMSetRenderTargets(1, &rtv, FALSE, nullptr);
    l->SetGraphicsRootSignature(comp_root);
    l->SetDescriptorHeaps(1, &srv_heap);
    uint32_t k[2] = {mode, 0};
    memcpy(&k[1], &white, 4);
    l->SetGraphicsRoot32BitConstants(0, 2, k, 0);
    l->SetGraphicsRootDescriptorTable(1, gsrv);
    l->SetPipelineState(pso);
    l->DrawInstanced(3, 1, 0, 0);
    transition(l, buf, D3D12_RESOURCE_STATE_RENDER_TARGET, t.state);
    if (ts_heap) {
        l->EndQuery(ts_heap, D3D12_QUERY_TYPE_TIMESTAMP, next * 2 + 1);
        l->ResolveQueryData(ts_heap, D3D12_QUERY_TYPE_TIMESTAMP, next * 2, 2, ts_buf, UINT64(next) * 16);
    }
    if (FAILED(l->Close()))
        return broken;
    ID3D12CommandList *lists[] = {l};
    render::d3d12::exec_original(q, 1, lists);
    q->Signal(fence, ++fence_val);
    s.done_at = fence_val;
    next = (next + 1) % SLOTS;
    return drawn;
}

void Renderer::shutdown()
{
    release(rtv_heap);
    release(srv_heap);
    release(vbuf);
    vmap = nullptr;
    for (auto &s : slots) {
        release(s.list);
        release(s.alloc);
    }
    release(fence);
    for (auto &p : psos)
        release(p.pso);
    psos.clear();
    for (auto &g : graveyard)
        g.second->Release();
    graveyard.clear();
    textures.release_all();
    release(layer);
    release(geo);
    release(ts_heap);
    release(ts_buf);
    release(root);
    release(comp_root);
    release(vs);
    release(ps);
    release(cvs);
    release(cps);
    release(dev);
    ready = false;
}

}
