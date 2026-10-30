// Native FFmpeg adapter. DLLs are loaded only from the executable directory.
#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <algorithm>
#include <vector>
#include <cstring>
extern "C" {
#include <libavcodec/avcodec.h>
#include <libavutil/hwcontext.h>
#include <libavutil/opt.h>
#include <libswscale/swscale.h>
}

struct Decoder {
    HMODULE util{}, codec{}, scale{};
#define FN(name) decltype(&name) name##_{};
    FN(avcodec_find_decoder_by_name) FN(avcodec_alloc_context3) FN(avcodec_open2)
    FN(avcodec_send_packet) FN(avcodec_receive_frame) FN(avcodec_free_context)
    FN(avcodec_get_hw_config) FN(avcodec_flush_buffers)
    FN(av_packet_alloc) FN(av_new_packet) FN(av_packet_unref) FN(av_packet_free)
    FN(av_frame_alloc) FN(av_frame_free) FN(av_frame_unref)
    FN(av_hwdevice_ctx_create) FN(av_hwframe_transfer_data) FN(av_buffer_ref)
    FN(av_buffer_unref) FN(sws_getCachedContext) FN(sws_scale) FN(sws_freeContext)
    FN(avcodec_version) FN(avutil_version)
#undef FN
    AVCodecContext* context{};
    AVPacket* packet{};
    AVFrame* frame{};
    AVFrame* cpu{};
    AVBufferRef* hardware{};
    AVPixelFormat hwformat{AV_PIX_FMT_NONE};
    SwsContext* converter{};
    std::vector<unsigned char> rgba;
    int width{}, height{}, source_width{}, source_height{};
    bool hardware_active{};
    ~Decoder() {
        if (converter) sws_freeContext_(converter);
        if (frame) av_frame_free_(&frame);
        if (cpu) av_frame_free_(&cpu);
        if (packet) av_packet_free_(&packet);
        if (context) avcodec_free_context_(&context);
        if (hardware) av_buffer_unref_(&hardware);
        if (scale) FreeLibrary(scale);
        if (codec) FreeLibrary(codec);
        if (util) FreeLibrary(util);
    }
};

static HMODULE load(const wchar_t* filename) {
    wchar_t path[32768];
    auto n=GetModuleFileNameW(nullptr,path,32768);
    if (!n || n>=32768) return nullptr;
    auto slash=wcsrchr(path,L'\\');
    if (!slash) return nullptr;
    wcscpy_s(slash+1,32768-(slash+1-path),filename);
    return LoadLibraryExW(path,nullptr,LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR|LOAD_LIBRARY_SEARCH_SYSTEM32);
}

static AVPixelFormat choose_format(AVCodecContext* c,const AVPixelFormat* formats) {
    auto d=static_cast<Decoder*>(c->opaque);
    for(auto p=formats;*p!=AV_PIX_FMT_NONE;++p) if(*p==d->hwformat) return *p;
    // Retain software decoding when the stream cannot use this device's hardware.
    for(auto p=formats;*p!=AV_PIX_FMT_NONE;++p)
        if(*p==AV_PIX_FMT_YUV420P || *p==AV_PIX_FMT_YUVJ420P) return *p;
    return formats[0];
}

extern "C" Decoder* video_create(int software) {
    auto d=new Decoder;
    d->util=load(L"avutil-61.dll"); d->codec=load(L"avcodec-63.dll"); d->scale=load(L"swscale-10.dll");
    if(!d->util||!d->codec||!d->scale) { delete d; return nullptr; }
#define GET(lib,name) d->name##_=reinterpret_cast<decltype(&name)>(GetProcAddress(d->lib,#name)); if(!d->name##_) {delete d;return nullptr;}
    GET(codec,avcodec_find_decoder_by_name) GET(codec,avcodec_alloc_context3) GET(codec,avcodec_open2)
    GET(codec,avcodec_send_packet) GET(codec,avcodec_receive_frame) GET(codec,avcodec_free_context)
    GET(codec,avcodec_get_hw_config) GET(codec,avcodec_flush_buffers)
    GET(codec,av_packet_alloc) GET(codec,av_new_packet) GET(codec,av_packet_unref) GET(codec,av_packet_free)
    GET(util,av_frame_alloc) GET(util,av_frame_free) GET(util,av_frame_unref)
    GET(util,av_hwdevice_ctx_create) GET(util,av_hwframe_transfer_data) GET(util,av_buffer_ref)
    GET(util,av_buffer_unref) GET(scale,sws_getCachedContext) GET(scale,sws_scale) GET(scale,sws_freeContext)
    GET(codec,avcodec_version) GET(util,avutil_version)
#undef GET
    if((d->avcodec_version_()>>16)!=LIBAVCODEC_VERSION_MAJOR || (d->avutil_version_()>>16)!=LIBAVUTIL_VERSION_MAJOR) {delete d;return nullptr;}
    auto codec=d->avcodec_find_decoder_by_name_("hevc");
    if(!codec) {delete d;return nullptr;}
    d->context=d->avcodec_alloc_context3_(codec);
    d->frame=d->av_frame_alloc_(); d->cpu=d->av_frame_alloc_(); d->packet=d->av_packet_alloc_();
    if(!d->context||!d->frame||!d->cpu||!d->packet) {delete d;return nullptr;}
    d->context->thread_count=2; d->context->thread_type=FF_THREAD_SLICE;
    if(!software) {
        for(int i=0;;++i) {
            auto config=d->avcodec_get_hw_config_(codec,i);
            if(!config) break;
            if(config->device_type==AV_HWDEVICE_TYPE_D3D11VA && (config->methods&AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX)) {
                if(d->av_hwdevice_ctx_create_(&d->hardware,config->device_type,nullptr,nullptr,0)>=0) {
                    d->hwformat=config->pix_fmt;
                    d->context->hw_device_ctx=d->av_buffer_ref_(d->hardware);
                    d->context->opaque=d;
                    d->context->get_format=choose_format;
                }
                break;
            }
        }
    }
    if(d->avcodec_open2_(d->context,codec,nullptr)<0) {delete d;return nullptr;}
    return d;
}

// Return 1 for a decoded image, 0 for no output, negative on codec failure.
extern "C" int video_decode(Decoder* d,const unsigned char* data,int size,int maxw,int maxh) {
    d->av_packet_unref_(d->packet);
    if(size<=0 || size>8*1024*1024 || d->av_new_packet_(d->packet,size)<0) return -1;
    memcpy(d->packet->data,data,size);
    int result=d->avcodec_send_packet_(d->context,d->packet);
    if(result<0) return result;
    int got=0;
    while((result=d->avcodec_receive_frame_(d->context,d->frame))>=0) {
        auto frame=d->frame;
        d->hardware_active=frame->format==d->hwformat && d->hwformat!=AV_PIX_FMT_NONE;
        if(d->hardware_active) {
            d->av_frame_unref_(d->cpu);
            if(d->av_hwframe_transfer_data_(d->cpu,frame,0)<0) return -2;
            // Preserve color range when transferring hardware frames.
            d->cpu->color_range=frame->color_range;
            frame=d->cpu;
        }
        if(frame->width<=0 || frame->height<=0 || frame->width>8192 || frame->height>8192) return -3;
        d->source_width=frame->width; d->source_height=frame->height;
        double factor=std::min({1.0,double(std::max(2,maxw))/frame->width,double(std::max(2,maxh))/frame->height});
        d->width=std::max(2,int(frame->width*factor)); d->height=std::max(2,int(frame->height*factor));
        d->converter=d->sws_getCachedContext_(d->converter,frame->width,frame->height,(AVPixelFormat)frame->format,
            d->width,d->height,AV_PIX_FMT_RGBA,SWS_BILINEAR,nullptr,nullptr,nullptr);
        if(!d->converter) return -4;
        d->rgba.resize(size_t(d->width)*d->height*4);
        uint8_t* output[]={d->rgba.data(),nullptr,nullptr,nullptr}; int stride[]={d->width*4,0,0,0};
        if(d->sws_scale_(d->converter,frame->data,frame->linesize,0,frame->height,output,stride)<=0) return -5;
        d->av_frame_unref_(d->frame); got=1;
    }
    return got;
}
extern "C" const unsigned char* video_pixels(Decoder* d,int* w,int* h,int* sw,int* sh,int* hardware) {
    *w=d->width;*h=d->height;*sw=d->source_width;*sh=d->source_height;*hardware=d->hardware_active;
    return d->rgba.data();
}
extern "C" void video_destroy(Decoder* d) {delete d;}
