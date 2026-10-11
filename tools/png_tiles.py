"""Bounded lossless PNG assembly for offline NAIP tiles; standard library only."""
import struct
import zlib

def decode(raw):
    if not raw.startswith(b'\x89PNG\r\n\x1a\n'):raise ValueError('Not PNG')
    pos=8;packed=bytearray();header=None;ended=False
    while pos+12<=len(raw):
        n=struct.unpack('>I',raw[pos:pos+4])[0];kind=raw[pos+4:pos+8];data=raw[pos+8:pos+8+n]
        if pos+12+n>len(raw) or zlib.crc32(kind+data)!=struct.unpack('>I',raw[pos+8+n:pos+12+n])[0]:raise ValueError('PNG checksum/truncation')
        if kind==b'IHDR':
            if header is not None or n!=13:raise ValueError('Invalid PNG header')
            header=struct.unpack('>IIBBBBB',data)
        elif kind==b'IDAT':packed.extend(data)
        elif kind==b'IEND':ended=True;break
        pos+=n+12
    if not ended or header is None:raise ValueError('Incomplete PNG')
    w,h,depth,kind,compression,filtering,interlace=header
    if not (1<=w<=2048 and 1<=h<=2048) or depth!=8 or kind not in (2,6) or compression or filtering or interlace:raise ValueError('Expected bounded noninterlaced 8-bit RGB/RGBA PNG')
    channels=3 if kind==2 else 4;stride=w*channels;expected=(stride+1)*h
    inflater=zlib.decompressobj();scan=inflater.decompress(packed,expected+1)
    if len(scan)!=expected or not inflater.eof or inflater.unused_data:raise ValueError('PNG decompression size mismatch')
    previous=bytearray(stride);pixels=bytearray()
    for y in range(h):
        mode=scan[y*(stride+1)];row=bytearray(scan[y*(stride+1)+1:(y+1)*(stride+1)])
        if mode>4:raise ValueError('Unknown PNG filter')
        for i in range(stride):
            a=row[i-channels] if i>=channels else 0;b=previous[i];c=previous[i-channels] if i>=channels else 0
            if mode==1:predict=a
            elif mode==2:predict=b
            elif mode==3:predict=(a+b)//2
            elif mode==4:
                p=a+b-c;pa,pb,pc=abs(p-a),abs(p-b),abs(p-c);predict=a if pa<=pb and pa<=pc else b if pb<=pc else c
            else:predict=0
            row[i]=(row[i]+predict)&255
        if channels==3:pixels.extend(row)
        else:
            for i in range(0,stride,4):
                if row[i+3]!=255:raise ValueError('Imagery tile has transparent/no-data pixels')
                pixels.extend(row[i:i+3])
        previous=row
    return w,h,bytes(pixels)

def encode(w,h,pixels):
    if len(pixels)!=w*h*3:raise ValueError('Pixel size mismatch')
    def chunk(kind,data):return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data))
    rows=b''.join(b'\0'+pixels[y*w*3:(y+1)*w*3] for y in range(h))
    return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',w,h,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(rows))+chunk(b'IEND',b'')

def assemble(tiles,grid,tile_size=512):
    size=grid*tile_size
    if grid not in (1,2,4) or len(tiles)!=grid*grid or size>2048:raise ValueError('Invalid tile grid')
    pixels=bytearray(size*size*3)
    for index,raw in enumerate(tiles):
        w,h,data=decode(raw)
        if w!=tile_size or h!=tile_size:raise ValueError('Tile dimension mismatch')
        x=index%grid;y=index//grid
        for row in range(h):
            start=((y*h+row)*size+x*w)*3;pixels[start:start+w*3]=data[row*w*3:(row+1)*w*3]
    return encode(size,size,pixels)
