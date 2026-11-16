"""构造 WinZip AE-2 加密 + 指定压缩方法的 zip 夹具。

zip 规范里加密条目的 compression method 是 99，真实压缩方法放在 0x9901
extra field 里。本脚本按规范手工拼字节：7-Zip 与 pyzipper 都只写 AES+Store，
造不出这个组合，所以测试样本得自带。
"""
import hashlib
import hmac
import lzma
import os
import struct
import sys

from Crypto.Cipher import AES

ITERATIONS = 1000


def aes_ctr(key, data):
    """WinZip AES 用的是纯 CTR：16 字节小端计数器，从 1 开始。"""
    out = bytearray()
    counter = 1
    for i in range(0, len(data), 16):
        chunk = data[i:i + 16]
        ks = AES.new(key, AES.MODE_ECB).encrypt(counter.to_bytes(16, 'little'))
        counter += 1
        out += bytes(a ^ b for a, b in zip(chunk, ks))
    return bytes(out)


def lzma1_payload(payload):
    """ZIP 的 LZMA 载荷：版本(2) + 属性长度(2) + 属性(5) + LZMA1 裸流。"""
    props_byte = (2 * 5 + 0) * 9 + 3   # lc=3, lp=0, pb=2
    dict_size = 1 << 20
    filters = [{"id": lzma.FILTER_LZMA1, "lc": 3, "lp": 0, "pb": 2,
                "dict_size": dict_size}]
    comp = lzma.LZMACompressor(format=lzma.FORMAT_RAW, filters=filters)
    body = comp.compress(payload) + comp.flush()
    props = bytes([props_byte]) + struct.pack('<I', dict_size)
    return struct.pack('<HH', 9, 5) + props + body


def xz_payload(payload):
    """ZIP 的 XZ 载荷（method 95）：就是完整的 xz 容器。"""
    return lzma.compress(payload, format=lzma.FORMAT_XZ)


def build(path, password=b"hunter2", method=14, payload=b"hello from thzip"):
    raw = lzma1_payload(payload) if method == 14 else xz_payload(payload)

    salt = os.urandom(16)
    # PBKDF2-HMAC-SHA1 派生 2*key_len+2 字节：
    # [0:32] AES 密钥，[32:64] HMAC 密钥，[64:66] 密码验证值。
    key_len = 32
    dk = hashlib.pbkdf2_hmac('sha1', password, salt, ITERATIONS,
                             dklen=2 * key_len + 2)
    aes_key = dk[:key_len]
    auth_key = dk[key_len:2 * key_len]
    pwd_verify = dk[2 * key_len:]

    enc = aes_ctr(aes_key, raw)
    auth = hmac.new(auth_key, enc, hashlib.sha1).digest()[:10]
    entry = salt + pwd_verify + enc + auth

    # 0x9901: vendor_version(2)=2(AE-2), vendor_id(2)="AE", mode(1)=3(AES-256),
    #         actual compression method(2)
    extra = (struct.pack('<HH', 0x9901, 7)
             + struct.pack('<H', 0x0002)
             + b'AE'
             + struct.pack('<B', 3)
             + struct.pack('<H', method))

    name = b'hello.txt'
    crc = 0
    flags = 1  # bit 0: 加密

    out = bytearray()
    # local file header: 10 fields
    out += b'PK\x03\x04'
    out += struct.pack('<HHHHHIIIHH',
                       20, flags, 99, 0, 0,
                       crc, len(entry), len(payload),
                       len(name), len(extra))
    out += name + extra + entry

    cd_off = len(out)
    # central directory header: 16 fields
    out += b'PK\x01\x02'
    out += struct.pack('<HHHHHHIIIHHHHHII',
                       20, 20, flags, 99, 0, 0,
                       crc, len(entry), len(payload),
                       len(name), len(extra), 0,
                       0, 0, 0, 0)
    out += name + extra
    cd_size = len(out) - cd_off

    # end of central directory
    out += b'PK\x05\x06'
    out += struct.pack('<HHHHIIH', 0, 0, 1, 1, cd_size, cd_off, 0)
    open(path, 'wb').write(out)
    print(f'wrote {path} ({len(out)} bytes) method={method} '
          f'entry={len(entry)} payload={len(payload)}')


build(sys.argv[1], method=int(sys.argv[2]) if len(sys.argv) > 2 else 14)
