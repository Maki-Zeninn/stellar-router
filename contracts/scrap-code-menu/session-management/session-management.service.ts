import { Injectable, NotFoundException } from '@nestjs/common';
import { CreateSessionManagementDto } from './dto/create-session-management.dto';
import { UpdateSessionManagementDto } from './dto/update-session-management.dto';

@Injectable()
export class SessionManagementService {
  private readonly items: Array<{ id: string } & CreateSessionManagementDto> = [];

  findAll() {
    return this.items;
  }

  findOne(id: string) {
    const item = this.items.find((entry) => entry.id === id);
    if (!item) {
      throw new NotFoundException('SessionManagement item not found');
    }
    return item;
  }

  create(payload: CreateSessionManagementDto) {
    const created = { id: crypto.randomUUID(), ...payload };
    this.items.push(created);
    return created;
  }

  update(id: string, payload: UpdateSessionManagementDto) {
    const item = this.findOne(id);
    Object.assign(item, payload);
    return item;
  }

  remove(id: string) {
    const index = this.items.findIndex((entry) => entry.id === id);
    if (index === -1) {
      throw new NotFoundException('SessionManagement item not found');
    }
    this.items.splice(index, 1);
    return { id, deleted: true };
  }
}
