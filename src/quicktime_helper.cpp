// SPDX-License-Identifier: GPL-3.0-only
// Standalone USB capture process. stdout: u32 LE size + H.264 Annex-B access unit.
// stdin closure or a byte requests stop. No network, input injection or installation.
#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <libusb.h>
#include "Protocol/QuickTimeSession.h"
#include <array>
#include <chrono>
#include <cstdio>
#include <memory>
#include <stdexcept>
#include <thread>
#include <vector>
using namespace iPhoneMirror;
using Clock=std::chrono::steady_clock;
static void check(int result,const char* action) {
    if(result<0) throw std::runtime_error(std::string(action)+": "+libusb_error_name(result)+
        ". Reconnect the USB cable and retry. If this persists, use Developer Mode; this raw USB backend is not compatible with the current connection.");
}
static bool stopping() {
    DWORD bytes=0;
    return !PeekNamedPipe(GetStdHandle(STD_INPUT_HANDLE),nullptr,0,nullptr,&bytes,nullptr)||bytes!=0;
}
static void write_all(const void* data,DWORD size) {
    auto p=static_cast<const unsigned char*>(data);
    while(size) {DWORD n=0;if(!WriteFile(GetStdHandle(STD_OUTPUT_HANDLE),p,size,&n,nullptr)||!n) throw std::runtime_error("Viewer closed");p+=n;size-=n;}
}
struct Context {
    libusb_context* p{};
    Context() {check(libusb_init(&p),"USB initialization");}
    ~Context(){if(p)libusb_exit(p);}
};
struct DeviceList {
    libusb_device** p{}; ssize_t size{};
    explicit DeviceList(libusb_context* c){size=libusb_get_device_list(c,&p);check(int(size),"USB discovery");}
    ~DeviceList(){if(p)libusb_free_device_list(p,1);}
};
struct Connection {
    libusb_device_handle* p{};
    int interface_number=-1;
    unsigned char input{},output{};
    bool changed=false;
    ~Connection(){
        if(p){
            if(interface_number>=0)libusb_release_interface(p,interface_number);
            if(changed)libusb_control_transfer(p,0x40,0x52,0,0,nullptr,0,1000);
            libusb_close(p);
        }
    }
    void send(const std::vector<std::uint8_t>& b){
        size_t offset=0;
        while(offset<b.size()) {int n=0;check(libusb_bulk_transfer(p,output,const_cast<unsigned char*>(b.data()+offset),int(b.size()-offset),&n,1000),"USB write");if(n<=0)throw std::runtime_error("USB write stopped");offset+=n;}
    }
};
struct Endpoint {int config{},number{},alt{};unsigned char in{},out{};};
static std::optional<Endpoint> endpoint(libusb_device* d,int subclass) {
    libusb_device_descriptor dd{};check(libusb_get_device_descriptor(d,&dd),"USB descriptor");
    for(int c=0;c<dd.bNumConfigurations;++c){
        libusb_config_descriptor* raw{};
        if(libusb_get_config_descriptor(d,c,&raw)<0)continue;
        std::unique_ptr<libusb_config_descriptor,decltype(&libusb_free_config_descriptor)> conf(raw,libusb_free_config_descriptor);
        for(int i=0;i<raw->bNumInterfaces;++i)for(int a=0;a<raw->interface[i].num_altsetting;++a){
            const auto& iface=raw->interface[i].altsetting[a];
            if(iface.bInterfaceClass!=0xff||iface.bInterfaceSubClass!=subclass)continue;
            Endpoint e{raw->bConfigurationValue,iface.bInterfaceNumber,iface.bAlternateSetting};
            for(int k=0;k<iface.bNumEndpoints;++k){auto ep=iface.endpoint[k];if((ep.bmAttributes&3)!=2)continue;if(ep.bEndpointAddress&0x80)e.in=ep.bEndpointAddress;else e.out=ep.bEndpointAddress;}
            if(e.in&&e.out)return e;
        }
    }
    return {};
}
static void capture() {
    // UsbDk is installed explicitly by the user, never by the viewer.
    auto scm=OpenSCManagerW(nullptr,nullptr,SC_MANAGER_CONNECT);
    auto service=scm?OpenServiceW(scm,L"UsbDk",SERVICE_QUERY_STATUS):nullptr;
    if(service)CloseServiceHandle(service);
    if(scm)CloseServiceHandle(scm);
    if(!service)throw std::runtime_error("Without Developer Mode needs the UsbDk capture driver. It is not installed. Use Developer Mode, or install UsbDk with administrator approval and reconnect your iPhone.");
    Context context;
    check(libusb_set_option(context.p,LIBUSB_OPTION_USE_USBDK),"UsbDk backend");
    std::array<std::uint8_t,8> ports{};int port_count=0;std::uint8_t bus=0;
    Connection connection;
    {
        DeviceList devices(context.p);libusb_device* selected=nullptr;
        for(ssize_t i=0;i<devices.size;++i){
            libusb_device_descriptor d{};if(libusb_get_device_descriptor(devices.p[i],&d)<0||d.idVendor!=0x05ac)continue;
            if(!endpoint(devices.p[i],0xfe)&&!endpoint(devices.p[i],0x2a))continue;
            if(selected)throw std::runtime_error("Connect only one iPhone by USB.");selected=devices.p[i];
        }
        if(!selected)throw std::runtime_error("Connect and unlock your iPhone, then tap Trust on the phone.");
        bus=libusb_get_bus_number(selected);port_count=libusb_get_port_numbers(selected,ports.data(),int(ports.size()));
        if(port_count<=0)throw std::runtime_error("Cannot identify the iPhone USB port safely.");
        check(libusb_open(selected,&connection.p),"Open iPhone USB device");
        // Transient Apple capture USB configuration, undone at stop/replug.
        libusb_control_transfer(connection.p,0x40,0x52,0,2,nullptr,0,1000);
        libusb_close(connection.p);connection.p=nullptr;
    }
    const auto deadline=Clock::now()+std::chrono::seconds(12);
    while(Clock::now()<deadline&&!stopping()){
        DeviceList devices(context.p);
        for(ssize_t i=0;i<devices.size;++i){
            std::array<std::uint8_t,8> candidate{};
            if(libusb_get_bus_number(devices.p[i])!=bus||libusb_get_port_numbers(devices.p[i],candidate.data(),int(candidate.size()))!=port_count||candidate!=ports)continue;
            auto ep=endpoint(devices.p[i],0x2a);if(!ep)continue;
            check(libusb_open(devices.p[i],&connection.p),"Open capture interface");connection.changed=true;
            int config=0;check(libusb_get_configuration(connection.p,&config),"Read USB configuration");
            if(config!=ep->config)check(libusb_set_configuration(connection.p,ep->config),"Select capture configuration");
            check(libusb_claim_interface(connection.p,ep->number),"Claim capture interface");connection.interface_number=ep->number;
            if(ep->alt)check(libusb_set_interface_alt_setting(connection.p,ep->number,ep->alt),"Select capture interface");
            connection.input=ep->in;connection.output=ep->out;break;
        }
        if(connection.p)break;
        std::this_thread::sleep_for(std::chrono::milliseconds(150));
    }
    if(!connection.p)throw std::runtime_error("QuickTime USB interface did not appear. Unplug and reconnect the iPhone. This mode has not yet been verified on your device.");
    quicktime::SessionOptions options;options.request_audio=false;options.demo_mode=false;
    quicktime::SessionProtocol protocol(options);quicktime::StreamDecoder parser;
    std::vector<std::uint8_t> buffer(256*1024),annex;
    auto last=Clock::now();
    bool recovered=false;
    while(!stopping()){
        int count=0;int result=libusb_bulk_transfer(connection.p,connection.input,buffer.data(),int(buffer.size()),&count,250);
        if(!recovered && protocol.video_frames()==0 && (result==LIBUSB_ERROR_IO || result==LIBUSB_ERROR_PIPE || (result==LIBUSB_ERROR_TIMEOUT && Clock::now()-last>std::chrono::seconds(1)))) {
            recovered=true;
            libusb_clear_halt(connection.p,connection.input);
            libusb_clear_halt(connection.p,connection.output);
            libusb_control_transfer(connection.p,0x40,0x40,0x6400,0x6400,nullptr,0,1000);
            connection.send(quicktime::make_ping());
            continue;
        }
        if(result!=LIBUSB_ERROR_TIMEOUT)check(result,"Read USB capture");
        if(Clock::now()-last>std::chrono::seconds(20))throw std::runtime_error("No video from iPhone. Unlock it, accept Trust if asked, and reconnect.");
        if(count<=0)continue;
        for(const auto& packet:parser.push(std::span(buffer.data(),size_t(count)))){
            auto event=protocol.process(packet);
            for(const auto& reply:event.outbound)connection.send(reply);
            if(!event.video_sample||event.video_sample->sample_data.empty())continue;
            const auto& format=protocol.video_format();
            if(!format||format->video_codec()!=coremedia::VideoCodec::H264)throw std::runtime_error("QuickTime did not negotiate supported H.264 video.");
            annex.clear();
            const auto append=[&](std::span<const std::uint8_t> bytes){
                if(annex.size()+bytes.size()+4>8*1024*1024)throw std::runtime_error("Video packet exceeds limit");
                annex.insert(annex.end(),{0,0,0,1});annex.insert(annex.end(),bytes.begin(),bytes.end());
            };
            for(const auto& s:format->sequence_parameter_sets)append(s);
            for(const auto& p:format->picture_parameter_sets)append(p);
            const auto& bytes=event.video_sample->sample_data;size_t offset=0;
            const size_t nalu=format->nalu_length_size;
            if(nalu<1||nalu>4)throw std::runtime_error("Invalid H.264 length size");
            while(offset<bytes.size()){
                if(bytes.size()-offset<nalu)throw std::runtime_error("Truncated H.264 size");
                size_t length=0;for(size_t i=0;i<nalu;++i)length=(length<<8)|bytes[offset++];
                if(!length||length>bytes.size()-offset)throw std::runtime_error("Truncated H.264 frame");
                append(std::span(bytes.data()+offset,length));offset+=length;
            }
            auto length=std::uint32_t(annex.size());write_all(&length,4);write_all(annex.data(),length);last=Clock::now();
        }
    }
    for(const auto& b:protocol.stop_messages())connection.send(b);
    for(const auto& b:protocol.complete_stop_messages())connection.send(b);
}
static void self_test(){
    quicktime::StreamDecoder parser;
    const auto ping=quicktime::make_ping();
    if(!parser.push(std::span(ping.data(),size_t(3))).empty())throw std::runtime_error("Partial packet emitted");
    const auto packets=parser.push(std::span(ping.data()+3,ping.size()-3));
    if(packets.size()!=1||packets[0].kind!=quicktime::PacketKind::Ping)throw std::runtime_error("Fragmented ping failed");
    quicktime::SessionProtocol protocol;
    if(protocol.process(packets[0]).outbound.empty())throw std::runtime_error("Missing handshake reply");
    parser.reset();bool rejected=false;
    const std::array<std::uint8_t,4> oversized{255,255,255,127};
    try{(void)parser.push(oversized);}catch(const std::exception&){rejected=true;}
    if(!rejected)throw std::runtime_error("Oversized packet accepted");
    std::fprintf(stderr,"QuickTime parser tests passed\n");
}
int main(int argc,char** argv){try{if(argc==2&&std::string(argv[1])=="--self-test")self_test();else capture();return 0;}catch(const std::exception& e){std::fprintf(stderr,"%s\n",e.what());return 1;}}
