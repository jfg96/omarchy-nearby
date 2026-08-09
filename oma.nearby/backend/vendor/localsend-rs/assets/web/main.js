(async () => {
  const query = new URLSearchParams(location.search)
  const pin = query.get('pin')
  const endpoint = `/api/localsend/v2/prepare-download${pin ? `?pin=${encodeURIComponent(pin)}` : ''}`
  const response = await fetch(endpoint, { method: 'POST' })
  const status = document.querySelector('#status')
  if (!response.ok) { status.textContent = `Unable to prepare download (${response.status})`; return }
  const data = await response.json()
  status.textContent = `${data.info.alias} shared:`
  const list = document.querySelector('#files')
  Object.entries(data.files).forEach(([id, file]) => {
    const item = document.createElement('li')
    const link = document.createElement('a')
    link.textContent = file.fileName
    link.href = `/api/localsend/v2/download?sessionId=${encodeURIComponent(data.sessionId)}&fileId=${encodeURIComponent(id)}`
    item.append(link)
    list.append(item)
  })
})()
