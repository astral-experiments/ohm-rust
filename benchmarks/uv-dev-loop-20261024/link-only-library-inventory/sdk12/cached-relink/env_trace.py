"""Private -v exec environment decoder; existing record/string validation retained."""
import re

def need(condition, message):
    if not condition:
        raise ValueError(message)

def _cstring(text, index):
    need(index < len(text) and text[index] == '"', 'expected complete trace string')
    index += 1
    value = bytearray()
    escapes = {'a': 7, 'b': 8, 'f': 12, 'n': 10, 'r': 13, 't': 9, 'v': 11, '\\': 92, '"': 34}
    while index < len(text) and text[index] != '"':
        character = text[index]
        index += 1
        if character != '\\':
            value.extend(character.encode('utf-8'))
            continue
        need(index < len(text), 'truncated trace escape')
        character = text[index]
        index += 1
        if character in escapes:
            value.append(escapes[character])
        elif character in '01234567':
            digits = character
            while index < len(text) and len(digits) < 3 and text[index] in '01234567':
                digits += text[index]
                index += 1
            need(int(digits, 8) <= 255, 'invalid trace octal escape')
            value.append(int(digits, 8))
        elif character == 'x':
            digits = text[index:index + 2]
            need(re.fullmatch('[0-9a-fA-F]{2}', digits), 'invalid trace hex escape')
            value.append(int(digits, 16))
            index += 2
        else:
            raise ValueError('unknown trace string escape')
    need(index < len(text), 'truncated trace string')
    need(0 not in value, 'NUL in exec argument')
    return value.decode('utf-8', errors='strict'), index + 1

def _exec(line):
    match = re.match(r'^(execve|execveat)\(', line)
    need(match is not None, 'unrecognized exec record')
    kind, index = match[1], match.end()
    if kind == 'execveat':
        directory = re.match(r'(?:AT_FDCWD|-?\d+),\s*', line[index:])
        need(directory is not None, 'ambiguous execveat directory')
        index += directory.end()
    path, index = _cstring(line, index)
    start = re.match(r',\s*\[', line[index:])
    need(start is not None, 'missing trace argv array')
    index += start.end()
    argv = []
    while index < len(line) and line[index] != ']':
        argument, index = _cstring(line, index)
        argv.append(argument)
        if index < len(line) and line[index] == ']':
            break
        separator = re.match(r',\s*', line[index:])
        need(separator is not None, 'truncated/ambiguous trace argv')
        index += separator.end()
    need(index < len(line) and line[index] == ']', 'unterminated trace argv')
    start = re.match(r',\s*\[', line[index + 1:])
    need(start is not None, 'missing complete trace environment array')
    index += 1 + start.end()
    environment = []
    while index < len(line) and line[index] != ']':
        value, index = _cstring(line, index)
        environment.append(value)
        if index < len(line) and line[index] == ']':
            break
        separator = re.match(r',\s*', line[index:])
        need(separator is not None, 'truncated/ambiguous trace environment')
        index += separator.end()
    need(index < len(line) and line[index] == ']', 'unterminated trace environment')
    tail = ''
    if kind == 'execveat':
        tail += r',\s*(?:0|AT_EMPTY_PATH|AT_SYMLINK_NOFOLLOW)(?:\|(?:AT_EMPTY_PATH|AT_SYMLINK_NOFOLLOW))*'
    tail += r'\s*\)\s*=\s*(0|-1 [A-Z0-9_]+ \([^\n]*\))\s*'
    finish = re.fullmatch(tail, line[index + 1:])
    need(finish is not None, 'truncated/ambiguous exec result or environment')
    successful = finish[1] == '0'
    need(not successful or bool(path), 'successful execveat has no auditable path')
    return {'executable': path, 'argv': argv, 'successful': successful, 'environment': environment}

def trace_records(text):
    pending, records = {}, []
    for line in text.split('\n'):
        if not line:
            continue
        prefix = re.match(r'^(?:\[pid\s+(\d+)\]|(\d+))\s+(.*)$', line)
        pid, body = ((prefix[1] or prefix[2]), prefix[3]) if prefix else (None, line)
        if body.endswith('<unfinished ...>'):
            match = re.match(r'^(execve|execveat)\(', body)
            need(pid is not None and pid not in pending and match is not None, 'ambiguous unfinished exec record')
            pending[pid] = (match[1], body[:-len('<unfinished ...>')])
            continue
        resumed = re.match(r'^<\.\.\. (execve|execveat) resumed>(.*)$', body)
        if resumed:
            need(pid in pending and pending[pid][0] == resumed[1], 'orphan or mismatched exec resume')
            body = pending.pop(pid)[1] + resumed[2]
        else:
            need(pid not in pending, 'new record before unfinished exec completes')
        if body.startswith(('execve(', 'execveat(')):
            records.append(_exec(body))
        else:
            need(re.fullmatch(r'--- SIG(?:[A-Z0-9]+|RT_[0-9]+) \{.*\} ---|\+\+\+ exited with \d+ \+\+\+', body), 'unrecognized trace record')
    need(not pending, 'unfinished exec record remains')
    return records
