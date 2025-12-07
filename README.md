# GoNhanh (Gõ Nhanh)

[![CI](https://github.com/khaphanspace/gonhanh.org/actions/workflows/ci.yml/badge.svg)](https://github.com/khaphanspace/gonhanh.org/actions/workflows/ci.yml)
[![License: GPL-3.0](https://img.shields.io/badge/License-GPL--3.0-blue.svg)](LICENSE)

**GoNhanh** (đọc là **Gõ Nhanh**) - Bộ gõ tiếng Việt hiệu suất cao, hiện đại và đa nền tảng.

Sức mạnh của **Rust** (Core Engine) kết hợp với **Native UI** (SwiftUI) mang lại trải nghiệm gõ phím mượt mà, ổn định và bảo mật tuyệt đối.

## Về dự án

GoNhanh được xây dựng với mục tiêu trở thành bộ gõ tiếng Việt **hoàn thiện nhất** về trải nghiệm người dùng và hiệu năng kỹ thuật:

- **Hiệu suất vượt trội**: Core engine viết bằng Rust, xử lý tức thì, không độ trễ.
- **Đa nền tảng**: Kiến trúc Hybrid thông minh, sẵn sàng cho macOS, Windows và Linux.
- **Hiện đại**: Hỗ trợ Unicode chuẩn, logic đặt dấu ngữ âm học, không phụ thuộc bảng mã cũ.
- **Trải nghiệm mượt mà**: Giao diện thích ứng (Light/Dark mode), hoạt ảnh mượt mà, phản hồi lập tức.
- **Cài là dùng**: Cấu hình tối ưu sẵn (Smart Defaults), không cần thiết lập phức tạp.

## Động lực

Dự án kế thừa cảm hứng từ:

- Sự ổn định qua thập kỷ của **UniKey**.
- Tinh thần mã nguồn mở của **OpenKey**.
- Sự tối giản, hiệu quả của **EVKey**.

Xuất phát từ nhu cầu cá nhân về sự ổn định tuyệt đối trên các công cụ hiện đại (như Claude Code, Terminal), tôi (**Kha Phan**) phát triển GoNhanh để tiếp nối di sản của những người đi trước.

Đây là dự án **phi lợi nhuận**, được phát triển và duy trì bởi cá nhân tôi với cam kết: **Mã nguồn mở - Miễn phí - Của cộng đồng**.

## Ba Không

- 🚫 **Không thu phí**: Miễn phí trọn đời, không có bản "Premium".
- 🚫 **Không rác**: Không quảng cáo, không popup, không tính năng thừa thãi.
- 🚫 **Không theo dõi**: Offline 100%, không thu thập dữ liệu, mã nguồn minh bạch.

## Triết lý sản phẩm

Chúng tôi xây dựng GoNhanh dựa trên 3 trụ cột:

1.  **Chuẩn hóa**: Tuyệt đối tuân thủ quy tắc chính tả chữ Quốc ngữ (theo chuẩn BGD&ĐT).
2.  **Hiện đại**: Tận dụng sức mạnh công nghệ mới (Rust, Native UI) để tối ưu hiệu suất, bỏ qua các gánh nặng lịch sử (legacy code).
3.  **Tương lai**: Sẵn sàng cho các nền tảng và kiến trúc hệ điều hành thế hệ mới (Wayland, macOS updates).

## So sánh

|                |      GoNhanh      |      EVKey      |      OpenKey      |   UniKey   |
| :------------- | :---------------: | :-------------: | :---------------: | :--------: |
| **Trạng thái** |   🟢 **Active**   | 🔴 Discontinued |  🟡 Maintenance   | 🟢 Stable  |
| **Nền tảng**   | macOS, Windows\*  | macOS, Windows  | macOS, Win, Linux |  Windows   |
| Năm ra mắt     |       2025        |      2018       |       2019        |    2000    |
| Miễn phí       |        ✅         |       ✅        |        ✅         |     ✅     |
| Open source    |        ✅         |       ✅        |        ✅         |     ⚠️     |
| Công nghệ      | **Rust + Native** |    C++ + Qt     |     C++ + Qt      |    C++     |
| Bảng mã        |    **Unicode**    |   Đa bảng mã    |    Đa bảng mã     | Đa bảng mã |

_\* Windows: đang trong lộ trình phát triển (Roadmap)._

Nếu cần chuyển mã hay dùng bảng mã cũ, dùng UniKey/EVKey/OpenKey.

## Cách hoạt động

Engine dựa trên ngữ âm học tiếng Việt thay vì bảng tra cứu:

```
Âm tiết = [Phụ âm đầu] + [Âm đệm] + Nguyên âm chính + [Âm cuối] + Thanh điệu
```

Thuật toán đặt dấu theo quy tắc ngữ âm. Hỗ trợ cả kiểu cũ (`hoà`) và kiểu mới (`hòa`).

Chi tiết: [docs/vietnamese-language-system.md](docs/vietnamese-language-system.md)

## Kiến trúc

```
┌─────────────────────────────────────┐
│         Platform UI Layer           │
│  ┌──────────┐      ┌──────────┐    │
│  │  macOS   │      │ Windows  │    │
│  │ SwiftUI  │      │   WPF    │    │
│  └─────┬────┘      └────┬─────┘    │
└────────┼────────────────┼──────────┘
         │    FFI (C ABI) │
┌────────▼────────────────▼──────────┐
│         Rust Core Library          │
│  ┌─────────────────────────────┐   │
│  │  Engine (Telex/VNI)         │   │
│  │  - Buffer management        │   │
│  │  - Phonology-based rules    │   │
│  │  - Unicode output           │   │
│  └─────────────────────────────┘   │
└────────────────────────────────────┘
```

- macOS: SwiftUI (done)
- Windows: WPF (planned)

## Cài đặt

```bash
git clone https://github.com/khaphanspace/gonhanh.org
cd gonhanh.org
make build
cp -r platforms/macos/build/Release/GoNhanh.app /Applications/
```

Lần đầu chạy cần cấp quyền Accessibility trong System Settings.

## Phát triển

```bash
make test    # 99 tests
make build   # Build tất cả
make clean   # Xóa build artifacts
```

Xem thêm: [docs/development.md](docs/development.md) · [docs/architecture.md](docs/architecture.md)

## Roadmap

| Version | Khi nào | Gì                  |
| ------- | ------- | ------------------- |
| 0.1     | Q1 2025 | macOS beta          |
| 0.2     | Q2 2025 | Stable, auto-update |
| 0.3     | Q3 2025 | Windows             |
| 1.0     | Q4 2025 | Production          |

## Credits

- [UniKey](https://www.unikey.org/)
- [OpenKey](https://github.com/tuyenvm/OpenKey)
- [EVKey](https://evkeyvn.com/)

## License

[GPL-3.0-or-later](LICENSE)
