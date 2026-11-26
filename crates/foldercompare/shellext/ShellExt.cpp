// Explorer context menu handler for FolderCompare (classic menu, registered per user via ExplorerCommandHandler).
//
// CompareCommand    "Compare to "<left>" with FolderCompare" (1 item + stored left), "Compare Folders" /
//                   "Compare Files" (2 items of the same kind). Hidden otherwise.
// SelectLeftCommand "Select as left for FolderCompare" (exactly 1 item). Stores the item in the registry.
//
// Stored left:  HKCU\Software\FolderCompare\LeftSelection  Path (REG_SZ), IsFolder (REG_DWORD)
// Exe to start: HKCU\Software\FolderCompare                ExePath (REG_SZ)
// Both are written by FolderCompare.exe (ExplorerMenuLayout / ExplorerIntegrationService); keep the names in sync.

#include <windows.h>
#include <shlobj.h>
#include <shobjidl.h>
#include <shlwapi.h>
#include <wrl/module.h>
#include <wrl/implements.h>
#include <string>

using Microsoft::WRL::ClassicCom;
using Microsoft::WRL::InhibitRoOriginateError;
using Microsoft::WRL::Module;
using Microsoft::WRL::ModuleType;
using Microsoft::WRL::RuntimeClass;
using Microsoft::WRL::RuntimeClassFlags;

namespace
{
const wchar_t AppKey[] = L"Software\\FolderCompare";
const wchar_t LeftKey[] = L"Software\\FolderCompare\\LeftSelection";

enum class ItemType { Other, Folder, File };

struct LeftSelection
{
    std::wstring path;
    ItemType type = ItemType::Other;
};

bool ReadString(HKEY root, const wchar_t* subKey, const wchar_t* name, std::wstring& value)
{
    DWORD size = 0;
    if (RegGetValueW(root, subKey, name, RRF_RT_REG_SZ, nullptr, nullptr, &size) != ERROR_SUCCESS || size < sizeof(wchar_t))
        return false;
    std::wstring buffer(size / sizeof(wchar_t), L'\0');
    if (RegGetValueW(root, subKey, name, RRF_RT_REG_SZ, nullptr, buffer.data(), &size) != ERROR_SUCCESS)
        return false;
    buffer.resize(wcslen(buffer.c_str()));
    value = buffer;
    return !value.empty();
}

bool LoadLeftSelection(LeftSelection& left)
{
    DWORD isFolder = 0, size = sizeof(isFolder);
    if (!ReadString(HKEY_CURRENT_USER, LeftKey, L"Path", left.path)) return false;
    if (RegGetValueW(HKEY_CURRENT_USER, LeftKey, L"IsFolder", RRF_RT_REG_DWORD, nullptr, &isFolder, &size) != ERROR_SUCCESS)
        return false;
    left.type = isFolder ? ItemType::Folder : ItemType::File;
    return true;
}

bool SaveLeftSelection(const std::wstring& path, ItemType type)
{
    HKEY key;
    if (RegCreateKeyExW(HKEY_CURRENT_USER, LeftKey, 0, nullptr, 0, KEY_SET_VALUE, nullptr, &key, nullptr) != ERROR_SUCCESS)
        return false;
    DWORD isFolder = type == ItemType::Folder ? 1 : 0;
    bool ok = RegSetValueExW(key, L"Path", 0, REG_SZ, reinterpret_cast<const BYTE*>(path.c_str()),
                             static_cast<DWORD>((path.size() + 1) * sizeof(wchar_t))) == ERROR_SUCCESS
           && RegSetValueExW(key, L"IsFolder", 0, REG_DWORD, reinterpret_cast<const BYTE*>(&isFolder), sizeof(isFolder)) == ERROR_SUCCESS;
    RegCloseKey(key);
    return ok;
}

void ClearLeftSelection() { RegDeleteTreeW(HKEY_CURRENT_USER, LeftKey); }

bool LoadExePath(std::wstring& exe) { return ReadString(HKEY_CURRENT_USER, AppKey, L"ExePath", exe); }

Microsoft::WRL::ComPtr<IShellItem> ItemAt(IShellItemArray* items, DWORD index)
{
    Microsoft::WRL::ComPtr<IShellItem> item;
    if (FAILED(items->GetItemAt(index, &item))) item.Reset();
    return item;
}

/// File-system folder, or file (zip and similar containers count as files). Everything else is Other.
ItemType GetItemType(IShellItemArray* items, DWORD index)
{
    auto item = ItemAt(items, index);
    SFGAOF attributes = 0;
    if (!item || FAILED(item->GetAttributes(SFGAO_FILESYSTEM | SFGAO_FOLDER | SFGAO_STREAM, &attributes)))
        return ItemType::Other;
    if (!(attributes & SFGAO_FILESYSTEM)) return ItemType::Other;
    return (attributes & SFGAO_FOLDER) && !(attributes & SFGAO_STREAM) ? ItemType::Folder : ItemType::File;
}

std::wstring GetItemPath(IShellItemArray* items, DWORD index)
{
    std::wstring result;
    auto item = ItemAt(items, index);
    PWSTR path = nullptr;
    if (item && SUCCEEDED(item->GetDisplayName(SIGDN_FILESYSPATH, &path)))
    {
        result = path;
        CoTaskMemFree(path);
    }
    return result;
}

/// Path without a trailing separator, except for a drive root like C:\.
std::wstring Normalize(std::wstring path)
{
    while (path.size() > 3 && (path.back() == L'\\' || path.back() == L'/')) path.pop_back();
    return path;
}

bool PathsEqual(const std::wstring& a, const std::wstring& b)
{
    auto x = Normalize(a), y = Normalize(b);
    return CompareStringOrdinal(x.c_str(), static_cast<int>(x.size()), y.c_str(), static_cast<int>(y.size()), TRUE) == CSTR_EQUAL;
}

bool PathExists(const std::wstring& path) { return GetFileAttributesW(path.c_str()) != INVALID_FILE_ATTRIBUTES; }

std::wstring FileNameOf(const std::wstring& path)
{
    auto normalized = Normalize(path);
    if (normalized.size() <= 3) return normalized; // drive root, e.g. "C:\"
    auto pos = normalized.find_last_of(L"\\/");
    return pos == std::wstring::npos ? normalized : normalized.substr(pos + 1);
}

std::wstring Quote(const std::wstring& arg)
{
    // Paths cannot contain quotes. A trailing backslash (drive root) must be doubled before the closing quote.
    std::wstring result = L"\"" + arg;
    if (!arg.empty() && arg.back() == L'\\') result += L'\\';
    return result + L"\"";
}

/// Starts FolderCompare.exe "left" "right" --compare.
bool LaunchCompare(const std::wstring& left, const std::wstring& right)
{
    std::wstring exe;
    if (!LoadExePath(exe)) return false;
    std::wstring commandLine = Quote(exe) + L" " + Quote(left) + L" " + Quote(right) + L" --compare";
    STARTUPINFOW startup{ sizeof(startup) };
    PROCESS_INFORMATION process{};
    if (!CreateProcessW(exe.c_str(), commandLine.data(), nullptr, nullptr, FALSE, 0, nullptr, nullptr, &startup, &process))
        return false;
    CloseHandle(process.hThread);
    CloseHandle(process.hProcess);
    return true;
}

DWORD Count(IShellItemArray* items)
{
    DWORD count = 0;
    if (!items || FAILED(items->GetCount(&count))) return 0;
    return count;
}

/// Shared parts of both commands.
template <typename Derived>
class CommandBase : public RuntimeClass<RuntimeClassFlags<ClassicCom | InhibitRoOriginateError>, IExplorerCommand>
{
public:
    IFACEMETHODIMP GetIcon(IShellItemArray*, LPWSTR* icon) override
    {
        *icon = nullptr;
        std::wstring exe;
        if (!LoadExePath(exe)) return E_NOTIMPL;
        return SHStrDupW((exe + L",0").c_str(), icon);
    }
    IFACEMETHODIMP GetToolTip(IShellItemArray*, LPWSTR* tip) override { *tip = nullptr; return E_NOTIMPL; }
    IFACEMETHODIMP GetCanonicalName(GUID* guid) override { *guid = __uuidof(Derived); return S_OK; }
    IFACEMETHODIMP GetFlags(EXPCMDFLAGS* flags) override { *flags = ECF_DEFAULT; return S_OK; }
    IFACEMETHODIMP EnumSubCommands(IEnumExplorerCommand** commands) override { *commands = nullptr; return E_NOTIMPL; }
};
}

class __declspec(uuid("501F238C-E07D-479E-9123-448B4646A913")) CompareCommand final : public CommandBase<CompareCommand>
{
public:
    IFACEMETHODIMP GetTitle(IShellItemArray* items, LPWSTR* name) override
    {
        *name = nullptr;
        std::wstring title;
        if (Count(items) == 2)
        {
            title = GetItemType(items, 0) == ItemType::Folder ? L"Compare Folders" : L"Compare Files";
        }
        else
        {
            LeftSelection left;
            if (!LoadLeftSelection(left)) return E_FAIL; // never show a title without a left item
            title = L"Compare to \"" + FileNameOf(left.path) + L"\" with FolderCompare";
        }
        return SHStrDupW(title.c_str(), name);
    }

    IFACEMETHODIMP GetState(IShellItemArray* items, BOOL okToBeSlow, EXPCMDSTATE* state) override
    {
        *state = ECS_HIDDEN;
        DWORD count = Count(items);

        if (count == 2)
        {
            auto t0 = GetItemType(items, 0);
            auto t1 = GetItemType(items, 1);
            if (t0 == t1 && t0 != ItemType::Other) *state = ECS_ENABLED; // F / G
            return S_OK;
        }

        if (count == 1)
        {
            LeftSelection left;
            if (!LoadLeftSelection(left)) return S_OK;                          // A
            if (!okToBeSlow && PathIsNetworkPathW(left.path.c_str())) return E_PENDING;
            if (!PathExists(left.path)) { ClearLeftSelection(); return S_OK; }  // E
            if (PathsEqual(GetItemPath(items, 0), left.path)) return S_OK;      // C
            if (GetItemType(items, 0) != left.type) return S_OK;               // D
            *state = ECS_ENABLED;                                               // B
        }
        return S_OK; // H, I
    }

    IFACEMETHODIMP Invoke(IShellItemArray* items, IBindCtx*) override
    {
        DWORD count = Count(items);
        std::wstring left, right;
        if (count == 2)
        {
            // Explorer's order: first item = left, second item = right.
            left = GetItemPath(items, 0);
            right = GetItemPath(items, 1);
        }
        else if (count == 1)
        {
            LeftSelection stored;
            if (!LoadLeftSelection(stored)) return E_FAIL;
            left = stored.path;
            right = GetItemPath(items, 0);
        }
        if (left.empty() || right.empty()) return E_FAIL;
        if (!LaunchCompare(left, right)) return HRESULT_FROM_WIN32(GetLastError());
        ClearLeftSelection(); // a comparison was started: reset
        return S_OK;
    }
};

class __declspec(uuid("0E804BB4-FBFB-4028-B0C3-E348EF8E0F71")) SelectLeftCommand final : public CommandBase<SelectLeftCommand>
{
public:
    IFACEMETHODIMP GetTitle(IShellItemArray*, LPWSTR* name) override
    {
        return SHStrDupW(L"Select as left for FolderCompare", name);
    }

    IFACEMETHODIMP GetState(IShellItemArray* items, BOOL, EXPCMDSTATE* state) override
    {
        *state = Count(items) == 1 && GetItemType(items, 0) != ItemType::Other ? ECS_ENABLED : ECS_HIDDEN;
        return S_OK;
    }

    IFACEMETHODIMP Invoke(IShellItemArray* items, IBindCtx*) override
    {
        if (Count(items) != 1) return E_FAIL;
        auto type = GetItemType(items, 0);
        auto path = GetItemPath(items, 0);
        if (type == ItemType::Other || path.empty()) return E_FAIL;
        return SaveLeftSelection(path, type) ? S_OK : E_FAIL;
    }
};

CoCreatableClass(CompareCommand)
CoCreatableClass(SelectLeftCommand)

STDAPI DllGetClassObject(REFCLSID clsid, REFIID riid, void** instance)
{
    return Module<ModuleType::InProc>::GetModule().GetClassObject(clsid, riid, instance);
}

STDAPI DllCanUnloadNow()
{
    return Module<ModuleType::InProc>::GetModule().GetObjectCount() == 0 ? S_OK : S_FALSE;
}

BOOL APIENTRY DllMain(HINSTANCE instance, DWORD reason, LPVOID)
{
    if (reason == DLL_PROCESS_ATTACH) DisableThreadLibraryCalls(instance);
    return TRUE;
}
