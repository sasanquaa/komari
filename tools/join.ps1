# https://gist.github.com/jehugaleahsa/e23d90f65f378aff9aa254e774b40bc7

function join($path, $destinationPath)
{
    $files = Get-ChildItem -Path "$path.*.part" | Sort-Object -Property @{Expression={
        $shortName = [System.IO.Path]::GetFileNameWithoutExtension($_.Name)
        $extension = [System.IO.Path]::GetExtension($shortName)
        if ($extension -ne $null -and $extension -ne '')
        {
            $extension = $extension.Substring(1)
        }
        [System.Convert]::ToInt32($extension)
    }}
    $writer = [System.IO.File]::OpenWrite($destinationPath)
    foreach ($file in $files)
    {
        $bytes = [System.IO.File]::ReadAllBytes($file)
        $writer.Write($bytes, 0, $bytes.Length)
    }
    $writer.Close()
}
