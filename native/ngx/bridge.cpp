// SPDX-License-Identifier: GPL-3.0-or-later
// VIctory's optional native NGX adapter. SDK files are supplied locally.
// The wire layout is the PC FidelityFX ABI used by victory-hle.
#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <d3d12.h>
#include <wrl/client.h>
#include <nvsdk_ngx_helpers.h>
#include <atomic>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <filesystem>
#include <memory>
#include <mutex>
#include <string>
#include <utility>

using Microsoft::WRL::ComPtr;
namespace {
constexpr uint32_t OK=0, RUNTIME=3, NO_PROVIDER=4, PARAMETER=6, UNSUPPORTED=7;
constexpr uint64_t CREATE=0x10000, DISPATCH=0x10001, VERSION=0x1000b;
constexpr uint64_t PROVIDER_ID=0x564943544e475802ull;
constexpr char PROVIDER_NAME[]="VIctory experimental NGX DLSS SR";
struct Header { uint64_t kind; Header* next; };
struct Size { uint32_t w,h; };
struct Create { Header header; uint32_t flags; Size render,output; uintptr_t message; };
struct Backend { Header header; ID3D12Device* device; };
struct Resource { ID3D12Resource* ptr; uint32_t description[8]; uint32_t state; };
struct Dispatch {
    Header header; ID3D12GraphicsCommandList* list; Resource res[7];
    float jitter[2], motion[2]; Size render,output;
    uint32_t sharpen; float sharpness,frame_ms,pre_exposure;
    uint32_t reset; float near_plane,far_plane,fov,meters; uint32_t flags;
};
struct Versions { Header header; uint64_t create_kind; ID3D12Device* device; uint64_t* count; uint64_t* ids; const char** names; };
struct ProviderVersion { Header header; uint64_t id; const char* name; };
struct Phase { Header header; uint32_t render,output; int32_t* count; };
struct Jitter { Header header; int32_t index,phases; float *x,*y; };
struct Ratio { Header header; uint32_t quality; float* out; };
struct Resolution { Header header; uint32_t w,h,quality; uint32_t *rw,*rh; };
struct Memory { Header header; uint64_t* usage; };
static_assert(sizeof(Create)==0x30 && sizeof(Resource)==0x30 && sizeof(Dispatch)==0x1b0);
static_assert(offsetof(Dispatch,jitter)==0x168 && offsetof(Dispatch,flags)==0x1ac);

std::mutex gate;
ID3D12Device* active_device=nullptr; // host owns device, provider stays loaded
std::wstring sdk_folder,data_folder;
std::string project_id;
const wchar_t* search_paths[1]{};
NVSDK_NGX_FeatureCommonInfo common{};
bool ok(NVSDK_NGX_Result r) { return NVSDK_NGX_SUCCEED(r); }
void log_result(const char* op,NVSDK_NGX_Result r) {
    std::fprintf(stderr,"VIctory NGX: %s (0x%08x)\n",op,unsigned(r));
}
bool same(Size a,Size b) { return a.w==b.w && a.h==b.h; }
bool valid(Size s) { return s.w && s.h && s.w<=16384 && s.h<=16384; }
bool within(Size s,Size lo,Size hi) {
    return valid(s) && s.w>=lo.w && s.h>=lo.h && s.w<=hi.w && s.h<=hi.h;
}
std::wstring env(const wchar_t* name) {
    DWORD n=GetEnvironmentVariableW(name,nullptr,0);
    if (!n) return {};
    std::wstring value(n,L'\0');
    DWORD written=GetEnvironmentVariableW(name,value.data(),n);
    if (!written || written>=n) return {};
    value.resize(written);
    return value;
}
bool guid(const std::wstring& s) {
    if (s.size()!=36) return false;
    for (size_t i=0;i<s.size();++i) {
        auto c=s[i];
        if (i==8 || i==13 || i==18 || i==23) { if (c!=L'-') return false; }
        else if (!((c>=L'0' && c<=L'9') || (c>=L'a' && c<=L'f') || (c>=L'A' && c<=L'F'))) return false;
    }
    return true;
}
bool initialize(ID3D12Device* device) {
    if (active_device) return active_device==device;
    if (!device || device->GetNodeCount()!=1) return false;
    auto project=env(L"VICTORY_NGX_PROJECT_ID"), data=env(L"VICTORY_NGX_DATA_DIR");
    if (!guid(project) || data.empty() || !std::filesystem::path(data).is_absolute()) {
        std::fprintf(stderr,"VIctory NGX: set your own VICTORY_NGX_PROJECT_ID UUID and absolute VICTORY_NGX_DATA_DIR\n");
        return false;
    }
    HMODULE module{};
    static int anchor;
    if (!GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                            reinterpret_cast<LPCWSTR>(&anchor),&module)) return false;
    std::wstring file(32768,L'\0');
    DWORD n=GetModuleFileNameW(module,file.data(),DWORD(file.size()));
    if (!n || n>=file.size()) return false;
    file.resize(n);
    sdk_folder=std::filesystem::path(file).parent_path().wstring();
    if (!std::filesystem::is_regular_file(std::filesystem::path(sdk_folder)/L"nvngx_dlss.dll")) return false;
    std::filesystem::create_directories(data);
    data_folder=data;
    project_id.assign(project.begin(),project.end());
    search_paths[0]=sdk_folder.c_str();
    common.PathListInfo.Path=search_paths;
    common.PathListInfo.Length=1;
    auto r=NVSDK_NGX_D3D12_Init_with_ProjectID(project_id.c_str(),NVSDK_NGX_ENGINE_TYPE_CUSTOM,
        "VIctory-0.1",data_folder.c_str(),device,&common);
    if (!ok(r)) { log_result("init",r); return false; }
    NVSDK_NGX_Parameter* caps=nullptr;
    int available=0;
    r=NVSDK_NGX_D3D12_GetCapabilityParameters(&caps);
    bool supported=ok(r) && caps && ok(NVSDK_NGX_Parameter_GetI(caps,
        NVSDK_NGX_Parameter_SuperSampling_Available,&available)) && available;
    if (caps) NVSDK_NGX_D3D12_DestroyParameters(caps);
    if (!supported) {
        NVSDK_NGX_D3D12_Shutdown1(device);
        std::fprintf(stderr,"VIctory NGX: DLSS SR unavailable\n");
        return false;
    }
    active_device=device;
    return true;
}
struct Context {
    std::atomic<uint32_t> refs{1};
    uint32_t flags{};
    Size render{},output{},min_render{};
    NVSDK_NGX_Handle* feature=nullptr;
    NVSDK_NGX_Parameter* params=nullptr;
    bool first=true;
    ~Context() {
        if (feature) NVSDK_NGX_D3D12_ReleaseFeature(feature);
        if (params) NVSDK_NGX_D3D12_DestroyParameters(params);
    }
};
Context* get(void** p) { return p ? static_cast<Context*>(*p) : nullptr; }
int feature_flags(uint32_t f) {
    int out=0;
    if (f&1) out|=NVSDK_NGX_DLSS_Feature_Flags_IsHDR;
    if (!(f&2)) out|=NVSDK_NGX_DLSS_Feature_Flags_MVLowRes;
    if (f&4) out|=NVSDK_NGX_DLSS_Feature_Flags_MVJittered;
    if (f&8) out|=NVSDK_NGX_DLSS_Feature_Flags_DepthInverted;
    if (f&32) out|=NVSDK_NGX_DLSS_Feature_Flags_AutoExposure;
    return out;
}
bool choose_quality(Context& ctx,NVSDK_NGX_PerfQuality_Value& quality) {
    NVSDK_NGX_Parameter* caps=nullptr;
    if (!ok(NVSDK_NGX_D3D12_GetCapabilityParameters(&caps)) || !caps) {
        if (caps) NVSDK_NGX_D3D12_DestroyParameters(caps);
        return false;
    }
    const NVSDK_NGX_PerfQuality_Value modes[]={
        NVSDK_NGX_PerfQuality_Value_MaxQuality,NVSDK_NGX_PerfQuality_Value_Balanced,
        NVSDK_NGX_PerfQuality_Value_MaxPerf,NVSDK_NGX_PerfQuality_Value_UltraPerformance,
        NVSDK_NGX_PerfQuality_Value_DLAA
    };
    double best=1e30;
    for (auto mode:modes) {
        Size optimal{},lo{},hi{}; float sharp{};
        auto r=NGX_DLSS_GET_OPTIMAL_SETTINGS(caps,ctx.output.w,ctx.output.h,mode,
            &optimal.w,&optimal.h,&hi.w,&hi.h,&lo.w,&lo.h,&sharp);
        if (!ok(r) || !within(ctx.render,lo,hi)) continue;
        double score=std::abs(double(ctx.render.w)-optimal.w)+std::abs(double(ctx.render.h)-optimal.h);
        if (score<best) { best=score; quality=mode; ctx.min_render=lo; }
    }
    NVSDK_NGX_D3D12_DestroyParameters(caps);
    return best<1e30;
}
// Create on an independent direct command list, with a completed fence
// before returning to the title. No title texture is involved here.
bool make_feature(std::unique_ptr<Context>& ctx) {
    NVSDK_NGX_PerfQuality_Value quality{};
    if (!choose_quality(*ctx,quality)) return false;
    if (!ok(NVSDK_NGX_D3D12_AllocateParameters(&ctx->params)) || !ctx->params) return false;
    auto* device=active_device;
    ComPtr<ID3D12CommandQueue> queue;
    ComPtr<ID3D12CommandAllocator> allocator;
    ComPtr<ID3D12GraphicsCommandList> list;
    ComPtr<ID3D12Fence> fence;
    D3D12_COMMAND_QUEUE_DESC q{}; q.Type=D3D12_COMMAND_LIST_TYPE_DIRECT;
    if (FAILED(device->CreateCommandQueue(&q,IID_PPV_ARGS(&queue))) ||
        FAILED(device->CreateCommandAllocator(q.Type,IID_PPV_ARGS(&allocator))) ||
        FAILED(device->CreateCommandList(0,q.Type,allocator.Get(),nullptr,IID_PPV_ARGS(&list))) ||
        FAILED(device->CreateFence(0,D3D12_FENCE_FLAG_NONE,IID_PPV_ARGS(&fence)))) return false;
    HANDLE event=CreateEventW(nullptr,FALSE,FALSE,nullptr);
    if (!event) return false;
    NVSDK_NGX_DLSS_Create_Params create{};
    create.Feature.InWidth=ctx->render.w; create.Feature.InHeight=ctx->render.h;
    create.Feature.InTargetWidth=ctx->output.w; create.Feature.InTargetHeight=ctx->output.h;
    create.Feature.InPerfQualityValue=quality;
    create.InFeatureCreateFlags=feature_flags(ctx->flags);
    auto r=NGX_D3D12_CREATE_DLSS_EXT(list.Get(),1,1,&ctx->feature,ctx->params,&create);
    if (!ok(r) || !ctx->feature || FAILED(list->Close()) || FAILED(fence->SetEventOnCompletion(1,event))) {
        log_result("create feature",r); CloseHandle(event); return false;
    }
    ID3D12CommandList* lists[]={list.Get()};
    queue->ExecuteCommandLists(1,lists);
    const HRESULT signal=queue->Signal(fence.Get(),1);
    const DWORD waited=SUCCEEDED(signal) ? WaitForSingleObject(event,INFINITE) : WAIT_FAILED;
    if (waited!=WAIT_OBJECT_0 || fence->GetCompletedValue()!=1) {
        // Completion unknown: retain queued objects until process exit.
        std::fprintf(stderr,"VIctory NGX: init fence failed; retaining pending resources\n");
        ctx.release(); queue.Detach(); allocator.Detach(); list.Detach(); fence.Detach();
        return false;
    }
    CloseHandle(event);
    std::fprintf(stderr,"VIctory NGX: created %ux%u -> %ux%u (quality %d)\n",
        ctx->render.w,ctx->render.h,ctx->output.w,ctx->output.h,int(quality));
    return true;
}
bool texture(const Resource& r,Size size,bool output) {
    if (!r.ptr || (output ? r.state!=2 : (r.state!=4 && r.state!=12))) return false;
    const auto d=r.ptr->GetDesc();
    return d.Dimension==D3D12_RESOURCE_DIMENSION_TEXTURE2D && d.DepthOrArraySize==1 &&
        d.SampleDesc.Count==1 && d.Width>=size.w && d.Height>=size.h &&
        (!output || (d.Flags&D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS));
}
float halton(uint32_t n,uint32_t base) {
    float result=0,weight=1;
    while (n) { weight/=float(base); result+=float(n%base)*weight; n/=base; }
    return result;
}
constexpr float ratios[]={1.0f,1.5f,1.7f,2.0f,3.0f};
} // namespace

#define EXPORT extern "C" __declspec(dllexport)
EXPORT uint32_t victoryStreamlineBridgeAbi() { return 2; }
EXPORT uint32_t victoryStreamlineBridgeRetain(void* p) {
    if (!p) return PARAMETER;
    auto& n=static_cast<Context*>(p)->refs;
    uint32_t current=n.load(std::memory_order_relaxed);
    do { if (!current || current==UINT32_MAX) return PARAMETER; }
    while (!n.compare_exchange_weak(current,current+1,std::memory_order_relaxed));
    return OK;
}
EXPORT void victoryStreamlineBridgeRelease(void* p) {
    if (!p) return;
    auto* ctx=static_cast<Context*>(p);
    if (ctx->refs.fetch_sub(1,std::memory_order_acq_rel)==1) {
        std::lock_guard<std::mutex> lock(gate);
        delete ctx;
    }
}
EXPORT uint32_t ffxCreateContext(void** out,Header* h,const void* memory) try {
    if (!out || !h || h->kind!=CREATE || memory) return PARAMETER;
    std::lock_guard<std::mutex> lock(gate);
    const auto& d=*reinterpret_cast<Create*>(h);
    if (!valid(d.render) || !valid(d.output) || !within(d.render,{1,1},d.output) || (d.flags&~127u))
        return UNSUPPORTED;
    ID3D12Device* device=nullptr;
    auto* next=h->next;
    for (unsigned i=0;next && i<8;++i,next=next->next) {
        if (next->kind==2) device=reinterpret_cast<Backend*>(next)->device;
        else if (next->kind!=VERSION) return UNSUPPORTED;
    }
    if (next || !active_device || active_device!=device) return PARAMETER;
    auto ctx=std::make_unique<Context>();
    ctx->flags=d.flags; ctx->render=d.render; ctx->output=d.output;
    if (!make_feature(ctx)) return RUNTIME;
    *out=ctx.release();
    return OK;
} catch (...) { return RUNTIME; }
EXPORT uint32_t ffxDestroyContext(void** p,const void* memory) {
    if (!get(p) || memory) return PARAMETER;
    auto* ctx=get(p); *p=nullptr;
    victoryStreamlineBridgeRelease(ctx);
    return OK;
}
EXPORT uint32_t ffxConfigure(void**,Header*) { return UNSUPPORTED; }
EXPORT uint32_t ffxQuery(void** p,Header* h) try {
    if (!h || h->next) return PARAMETER;
    std::lock_guard<std::mutex> lock(gate);
    switch (h->kind) {
    case 4: {
        auto& q=*reinterpret_cast<Versions*>(h);
        if (!q.count || q.create_kind!=CREATE || !q.device) return PARAMETER;
        if (!initialize(q.device)) { *q.count=0; return NO_PROVIDER; }
        if (*q.count==0) { *q.count=1; return OK; }
        *q.count=1;
        if (q.ids) q.ids[0]=PROVIDER_ID;
        if (q.names) q.names[0]=PROVIDER_NAME;
        return OK;
    }
    case 6: {
        if (!get(p)) return PARAMETER;
        auto& q=*reinterpret_cast<ProviderVersion*>(h);
        q.id=PROVIDER_ID; q.name=PROVIDER_NAME; return OK;
    }
    case 0x10002: {
        auto& q=*reinterpret_cast<Ratio*>(h);
        if (!q.out || q.quality>=5) return PARAMETER;
        *q.out=ratios[q.quality]; return OK;
    }
    case 0x10003: {
        auto& q=*reinterpret_cast<Resolution*>(h);
        if (!q.rw || !q.rh || !valid({q.w,q.h}) || q.quality>=5) return PARAMETER;
        *q.rw=uint32_t(q.w/ratios[q.quality]); *q.rh=uint32_t(q.h/ratios[q.quality]); return OK;
    }
    case 0x10004: {
        auto& q=*reinterpret_cast<Phase*>(h);
        if (!q.count || !q.render || !q.output || q.output>16384 || q.render>q.output) return PARAMETER;
        double ratio=double(q.output)/q.render;
        *q.count=int32_t(8*ratio*ratio); return OK;
    }
    case 0x10005: {
        auto& q=*reinterpret_cast<Jitter*>(h);
        if (!q.x || !q.y || q.index<0 || q.phases<=0) return PARAMETER;
        auto n=uint32_t(q.index%q.phases)+1;
        *q.x=halton(n,2)-.5f; *q.y=halton(n,3)-.5f; return OK;
    }
    case 0x10008: {
        auto& q=*reinterpret_cast<Memory*>(h);
        if (!get(p) || !q.usage) return PARAMETER;
        NVSDK_NGX_Parameter* caps=nullptr;
        if (!ok(NVSDK_NGX_D3D12_GetCapabilityParameters(&caps)) || !caps) {
            if (caps) NVSDK_NGX_D3D12_DestroyParameters(caps);
            return RUNTIME;
        }
        unsigned long long bytes=0;
        auto result=NGX_DLSS_GET_STATS(caps,&bytes);
        NVSDK_NGX_D3D12_DestroyParameters(caps);
        if (!ok(result)) return RUNTIME;
        q.usage[0]=bytes; q.usage[1]=0; return OK; // process total, not per context
    }
    default: return UNSUPPORTED;
    }
} catch (...) { return RUNTIME; }

EXPORT uint32_t ffxDispatch(void** p,Header* h) try {
    if (!get(p) || !h || h->kind!=DISPATCH || h->next) return PARAMETER;
    std::lock_guard<std::mutex> lock(gate);
    auto& ctx=*get(p);
    const auto& d=*reinterpret_cast<Dispatch*>(h);
    Size output=d.output.w==0 && d.output.h==0 ? ctx.output : d.output;
    if (!d.list || !same(output,ctx.output) || d.flags!=0 ||
        !within(d.render,ctx.min_render,ctx.render) ||
        (!(ctx.flags&64) && !same(d.render,ctx.render))) return UNSUPPORTED;
    for (float v:{d.jitter[0],d.jitter[1],d.motion[0],d.motion[1],d.pre_exposure,d.frame_ms})
        if (!std::isfinite(v)) return PARAMETER;
    if (d.pre_exposure<=0 || d.frame_ms<0 || d.motion[0]==0 || d.motion[1]==0 ||
        std::abs(d.jitter[0])>1 || std::abs(d.jitter[1])>1) return PARAMETER;
    const Size mv=ctx.flags&2 ? output : d.render;
    if (!texture(d.res[0],d.render,false) || !texture(d.res[1],d.render,false) ||
        !texture(d.res[2],mv,false) || !texture(d.res[6],output,true)) return PARAMETER;
    const bool auto_exposure=(ctx.flags&32)!=0;
    if (!auto_exposure && !texture(d.res[3],{1,1},false)) return PARAMETER;
    const unsigned used[]={0,1,2,3,6};
    for (unsigned i:used) {
        if (i==3 && auto_exposure) continue;
        for (unsigned j:used) {
            if (j>=i || (j==3 && auto_exposure)) continue;
            if (d.res[i].ptr==d.res[j].ptr) return PARAMETER;
        }
    }
    // Translate combined SRV state to non-pixel SRV, then restore it.
    D3D12_RESOURCE_BARRIER barriers[4]{};
    UINT count=0;
    for (unsigned i:{0u,1u,2u,3u}) {
        if ((i==3 && auto_exposure) || d.res[i].state!=12) continue;
        auto& b=barriers[count++];
        b.Type=D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;
        b.Transition.pResource=d.res[i].ptr;
        b.Transition.Subresource=D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES;
        b.Transition.StateBefore=D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE | D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE;
        b.Transition.StateAfter=D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE;
    }
    if (count) d.list->ResourceBarrier(count,barriers);
    NVSDK_NGX_D3D12_DLSS_Eval_Params eval{};
    eval.Feature.pInColor=d.res[0].ptr; eval.Feature.pInOutput=d.res[6].ptr;
    eval.pInDepth=d.res[1].ptr; eval.pInMotionVectors=d.res[2].ptr;
    eval.pInExposureTexture=auto_exposure ? nullptr : d.res[3].ptr;
    // FSR reactive/transparency masks are not equivalent to NGX bias masks.
    eval.InJitterOffsetX=d.jitter[0]; eval.InJitterOffsetY=d.jitter[1];
    eval.InMVScaleX=d.motion[0]; eval.InMVScaleY=d.motion[1];
    eval.InRenderSubrectDimensions.Width=d.render.w;
    eval.InRenderSubrectDimensions.Height=d.render.h;
    eval.InReset=ctx.first || (d.reset&0xff)!=0;
    eval.InPreExposure=d.pre_exposure; eval.InExposureScale=1.0f;
    eval.InFrameTimeDeltaInMsec=d.frame_ms;
    auto r=NGX_D3D12_EVALUATE_DLSS_EXT(d.list,ctx.feature,ctx.params,&eval);
    for (UINT i=0;i<count;++i)
        std::swap(barriers[i].Transition.StateBefore,barriers[i].Transition.StateAfter);
    if (count) d.list->ResourceBarrier(count,barriers);
    if (!ok(r)) { log_result("evaluate",r); return RUNTIME; }
    ctx.first=false;
    return OK;
} catch (...) { return RUNTIME; }
