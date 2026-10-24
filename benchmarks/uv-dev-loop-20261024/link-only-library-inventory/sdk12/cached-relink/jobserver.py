import array, fcntl, os, termios
jobfds = (5, 7)

def tokens():
 count=array.array('i',[0]);fcntl.ioctl(jobfds[0],termios.FIONREAD,count,True);return count[0]

def jobserver():
 for fd in jobfds:
  try:os.fstat(fd)
  except OSError as error:assert error.errno==9
  else:raise AssertionError(('desired jobserver FD already open',fd))
 r,w=os.pipe();high_r=fcntl.fcntl(r,fcntl.F_DUPFD_CLOEXEC,64);high_w=fcntl.fcntl(w,fcntl.F_DUPFD_CLOEXEC,64);os.close(r);os.close(w)
 os.dup2(high_r,jobfds[0],inheritable=True);os.dup2(high_w,jobfds[1],inheritable=True);os.close(high_r);os.close(high_w);os.write(jobfds[1],b'+'*7)
 assert tokens()==7 and os.fstat(jobfds[0]).st_ino==os.fstat(jobfds[1]).st_ino
 return {'fds':list(jobfds),'available_tokens':7,'implicit_job':1,'capacity':8,'read_access':fcntl.fcntl(jobfds[0],fcntl.F_GETFL)&os.O_ACCMODE,'write_access':fcntl.fcntl(jobfds[1],fcntl.F_GETFL)&os.O_ACCMODE}
